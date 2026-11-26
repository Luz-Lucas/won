fn main() {
    // Nome "WON" no Gerenciador de Tarefas (descrição do arquivo). Só no executável, só no Windows.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for("won.rc", ["sentinela"], embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
