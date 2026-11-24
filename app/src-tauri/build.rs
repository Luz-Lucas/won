fn main() {
    // O app sempre abre como administrador: o manifesto pede elevação ao
    // abrir (o Windows mostra o UAC). Só vale no Windows.
    let attrs = tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("sentinela.manifest")),
    );
    tauri_build::try_build(attrs).expect("falha ao gerar os recursos do Windows");
}
