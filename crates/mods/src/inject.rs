//! Carrega e descarrega uma DLL em um processo (LoadLibraryW / FreeLibrary remotos).

#[cfg(windows)]
mod imp {
    use anyhow::{Result, bail};
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE};
    use windows_sys::Win32::System::Diagnostics::Debug::WriteProcessMemory;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW, TH32CS_SNAPMODULE,
        TH32CS_SNAPMODULE32,
    };
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAllocEx, VirtualFreeEx,
    };
    use windows_sys::Win32::System::Threading::{
        CreateRemoteThread, GetExitCodeThread, INFINITE, IsWow64Process, OpenProcess,
        PROCESS_CREATE_THREAD, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ,
        PROCESS_VM_WRITE, WaitForSingleObject,
    };

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    fn wide(p: &Path) -> Vec<u16> {
        p.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    fn open(pid: u32) -> Result<Handle> {
        let access = PROCESS_CREATE_THREAD
            | PROCESS_QUERY_INFORMATION
            | PROCESS_VM_OPERATION
            | PROCESS_VM_READ
            | PROCESS_VM_WRITE;
        let h = unsafe { OpenProcess(access, FALSE, pid) };
        if h.is_null() {
            bail!("não foi possível abrir o processo {pid} (sem permissão ou já encerrado)");
        }
        let h = Handle(h);
        let mut wow = 0;
        if unsafe { IsWow64Process(h.0, &mut wow) } != 0 && wow != 0 {
            bail!("o processo {pid} é de 32 bits; só há suporte a processos de 64 bits");
        }
        Ok(h)
    }

    /// Roda `func(arg)` dentro do processo e devolve o código de saída da thread.
    fn remote_call(h: &Handle, func: usize, arg: *const core::ffi::c_void) -> Result<u32> {
        unsafe {
            let start: unsafe extern "system" fn(*mut core::ffi::c_void) -> u32 =
                core::mem::transmute(func);
            let t = CreateRemoteThread(
                h.0,
                core::ptr::null(),
                0,
                Some(start),
                arg,
                0,
                core::ptr::null_mut(),
            );
            if t.is_null() {
                bail!("CreateRemoteThread falhou");
            }
            let t = Handle(t);
            WaitForSingleObject(t.0, INFINITE);
            let mut code = 0;
            GetExitCodeThread(t.0, &mut code);
            Ok(code)
        }
    }

    fn kernel32(name: &[u8]) -> Result<usize> {
        let k: Vec<u16> = "kernel32.dll\0".encode_utf16().collect();
        unsafe {
            let m = GetModuleHandleW(k.as_ptr());
            match GetProcAddress(m, name.as_ptr()) {
                Some(f) => Ok(f as usize),
                None => bail!("função do kernel32 não encontrada"),
            }
        }
    }

    pub fn load(pid: u32, dll: &Path) -> Result<()> {
        let h = open(pid)?;
        let path = wide(dll);
        let bytes = path.len() * 2;
        unsafe {
            let mem = VirtualAllocEx(
                h.0,
                core::ptr::null(),
                bytes,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            );
            if mem.is_null() {
                bail!("VirtualAllocEx falhou");
            }
            let wrote =
                WriteProcessMemory(h.0, mem, path.as_ptr().cast(), bytes, core::ptr::null_mut());
            let result = if wrote == 0 {
                Err(anyhow::anyhow!("WriteProcessMemory falhou"))
            } else {
                remote_call(&h, kernel32(b"LoadLibraryW\0")?, mem)
            };
            VirtualFreeEx(h.0, mem, 0, MEM_RELEASE);
            if result? == 0 {
                bail!("o processo recusou carregar a DLL (LoadLibraryW devolveu nulo)");
            }
        }
        Ok(())
    }

    fn module_base(pid: u32, dll: &Path) -> Option<usize> {
        let want = dll.file_name()?.to_string_lossy().to_ascii_lowercase();
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid);
            if snap as isize == -1 {
                return None;
            }
            let snap = Handle(snap);
            let mut e: MODULEENTRY32W = core::mem::zeroed();
            e.dwSize = core::mem::size_of::<MODULEENTRY32W>() as u32;
            let mut ok = Module32FirstW(snap.0, &mut e);
            while ok != 0 {
                let n = e
                    .szModule
                    .iter()
                    .position(|c| *c == 0)
                    .unwrap_or(e.szModule.len());
                if String::from_utf16_lossy(&e.szModule[..n]).to_ascii_lowercase() == want {
                    return Some(e.modBaseAddr as usize);
                }
                ok = Module32NextW(snap.0, &mut e);
            }
        }
        None
    }

    /// Descarrega a DLL. Devolve `false` se ela já não estava no processo.
    pub fn unload(pid: u32, dll: &Path) -> Result<bool> {
        let Some(base) = module_base(pid, dll) else {
            return Ok(false);
        };
        let h = open(pid)?;
        remote_call(&h, kernel32(b"FreeLibrary\0")?, base as *const _)?;
        Ok(true)
    }
}

#[cfg(windows)]
pub use imp::{load, unload};

#[cfg(not(windows))]
pub fn load(_pid: u32, _dll: &std::path::Path) -> anyhow::Result<()> {
    anyhow::bail!("injeção só existe no Windows")
}

#[cfg(not(windows))]
pub fn unload(_pid: u32, _dll: &std::path::Path) -> anyhow::Result<bool> {
    anyhow::bail!("injeção só existe no Windows")
}
