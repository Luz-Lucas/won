param($Image,$PdbDir,$Out)
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices; using System.Collections.Generic; using System.Text;
public static class Dbg {
 [StructLayout(LayoutKind.Sequential)] public struct SYMBOL_INFO { public uint SizeOfStruct,TypeIndex; public ulong Reserved0,Reserved1; public uint Index,Size; public ulong ModBase; public uint Flags; public ulong Value,Address; public uint Register,Scope,Tag,NameLen,MaxNameLen; }
 public delegate bool EnumCb(IntPtr si, uint size, IntPtr ctx);
 [DllImport("dbghelp.dll",SetLastError=true,CharSet=CharSet.Ansi)] public static extern bool SymInitialize(IntPtr h,string path,bool inv);
 [DllImport("dbghelp.dll",SetLastError=true,CharSet=CharSet.Ansi)] public static extern ulong SymLoadModuleEx(IntPtr h,IntPtr f,string img,string mod,ulong b,uint sz,IntPtr d,uint fl);
 [DllImport("dbghelp.dll",SetLastError=true,CharSet=CharSet.Ansi)] public static extern bool SymEnumSymbols(IntPtr h,ulong b,string mask,EnumCb cb,IntPtr ctx);
 [DllImport("dbghelp.dll")] public static extern uint SymSetOptions(uint o);
 public static List<string> Run(string img,string path,ulong baseAddr){
  var res=new List<string>(); IntPtr h=(IntPtr)0x1234;
  SymSetOptions(0x2|0x4|0x10000|0x1000000); // UNDNAME? keep: CASE_INSENSITIVE no. use DEFERRED off
  if(!SymInitialize(h,path,false)) throw new Exception("init "+Marshal.GetLastWin32Error());
  ulong b=SymLoadModuleEx(h,IntPtr.Zero,img,null,baseAddr,0,IntPtr.Zero,0);
  if(b==0) throw new Exception("load "+Marshal.GetLastWin32Error());
  EnumCb cb=(si,size,ctx)=>{ var s=(SYMBOL_INFO)Marshal.PtrToStructure(si,typeof(SYMBOL_INFO)); string n=Marshal.PtrToStringAnsi(new IntPtr(si.ToInt64()+84),(int)s.NameLen); res.Add(String.Format("{0:X}\t{1}\t{2}",s.Address-b,s.Size,n)); return true;};
  SymEnumSymbols(h,b,"*",cb,IntPtr.Zero);
  return res; }
}
'@
$r=[Dbg]::Run($Image,$PdbDir,0x10000000)
$r | Set-Content -Encoding UTF8 $Out
"$($r.Count) simbolos"
