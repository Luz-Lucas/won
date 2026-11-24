# Roteiro de testes manuais — Fase 5 (Wallpaper Engine e Temas)

Execute num **PC com Windows 11**, com o Sentinela **instalado**, a **Steam** com o **Wallpaper Engine** (app 431960) e pelo menos três wallpapers assinados na Oficina (um de cena, um de vídeo e um GIF). Para o último bloco, um segundo monitor e um jogo que abra em tela cheia. Marque o resultado de cada item: **OK**, **FALHA** (descreva) ou **N/A**.

> Estado desta cópia: roteiro escrito, **ainda não executado**. Fora do Windows foram conferidos só a lógica pura (leitura da Steam e da Oficina, paleta, formato e validação do tema, aplicar e desfazer com motor e Wallpaper Engine falsos) e a compilação para Windows (`clippy` com o alvo `x86_64-pc-windows-msvc`). **Nada que fala com o Windows ou com o Wallpaper Engine foi executado.** Em especial, **a sintaxe das opções `-control` ainda não foi conferida num programa real** (passo 1.3): se falhar, é o primeiro lugar a ajustar (`crates/core/src/wallpaper/host.rs`).

| # | Passo | Esperado | Resultado | Observações |
| --- | --- | --- | --- | --- |
| 1 | **Detecção e controle** | | | |
| 1.1 | `cargo test -p sentinela-core -- --ignored real_install_and_control` | Imprime a instalação (pasta, `wallpaper64.exe`, pasta da Oficina, build) | | |
| 1.2 | Mesmo teste, com o Wallpaper Engine aberto | O wallpaper pausa e volta a tocar, sem janela de console piscando | | |
| 1.3 | Personalizar → Papel de parede → **Usar** em outro wallpaper | O papel de parede muda em todos os monitores; a miniatura vira "Em uso" | | |
| 1.4 | Desinstalar/renomear a pasta do Wallpaper Engine e reabrir o Sentinela | A seção "Papel de parede" some; Temas continua funcionando | | |
| 2 | **Galeria** | | | |
| 2.1 | Abrir a seção com a Oficina grande (dezenas de itens) | Abre em poucos segundos; miniaturas PNG/JPEG aparecem; GIF e itens sem miniatura mostram a letra inicial | | |
| 2.2 | Wallpaper com título vazio ou `project.json` incompleto | Aparece com o número da pasta como título; nada quebra | | |
| 2.3 | Conferir a pasta da Oficina antes e depois | Nada foi criado, alterado ou apagado nela nem no `config.json` do Wallpaper Engine | | |
| 3 | **Pausa em jogos** | | | |
| 3.1 | Opção "Pausar" (padrão); abrir um jogo detectado em tela cheia | O wallpaper pausa (a GPU/CPU do Wallpaper Engine cai no Gerenciador de Tarefas) | | |
| 3.2 | Fechar o jogo | O wallpaper volta a tocar sozinho em até uns 10 s | | |
| 3.3 | Opção "Parar"; abrir e fechar o jogo | A memória de vídeo é liberada ao abrir; ao fechar, **o wallpaper volta** (se não voltar, `play` não reabre depois de `stop`: anotar, pode precisar de `openWallpaper`) | | |
| 3.4 | Jogo aberto, trocar a opção para "Não mexer" | O wallpaper volta a tocar na hora | | |
| 3.5 | Jogo aberto com o wallpaper pausado, fechar o Sentinela (bandeja → Sair) | O wallpaper volta a tocar | | |
| 3.6 | Matar o `wallpaper64.exe` com o jogo aberto, fechar o jogo | Sem erro na janela; uma linha "Wallpaper Engine: …" no histórico de atividade | | |
| 4 | **Temas** | | | |
| 4.1 | Temas → **Ver mudanças** em "Minimalista" | Diálogo lista cada mudança (valor atual → novo) e quantos itens já estão assim; **nada foi alterado ainda** | | |
| 4.2 | Cancelar | Nada muda | | |
| 4.3 | Repetir e **Aplicar tema** | Ajustes aplicados; aviso de "Reinicie o Explorer" quando couber; aparece "Último tema aplicado" | | |
| 4.4 | Histórico (página) | Mostra os ajustes individuais como antes (o tema entra neles) | | |
| 4.5 | **Desfazer tema** | Tudo volta ao que era antes do tema, de uma vez | | |
| 4.6 | "Translúcido" com um wallpaper em uso na Oficina | A cor de destaque do Windows passa a ser a do wallpaper; sair e entrar no Windows para valer em tudo; Desfazer devolve a cor anterior | | |
| 4.7 | Aplicar o mesmo tema duas vezes | A segunda diz "Nada mudou" e não cria item novo | | |
| 4.8 | Tema com wallpaper (importado, passo 5.1), com outro wallpaper em uso → Aplicar → Desfazer | O wallpaper do tema entra; ao desfazer, **volta o que estava antes** | | |
| 5 | **Exportar e importar** | | | |
| 5.1 | Com um wallpaper da Oficina em uso e alguns ajustes aplicados: nomear → **Exportar o visual atual…** | Cria `Nome.sentinela-theme`; abrir como zip: só `theme.toml` | | |
| 5.2 | **Importar tema…** com esse arquivo (em outra conta ou depois de desfazer tudo) | Aparece na lista como "Importado"; **não** aplica sozinho; Ver mudanças → Aplicar reproduz o visual | | |
| 5.3 | Importar um tema cujo wallpaper você não assinou | Aviso com o link da Oficina; o tema aplica o resto e ignora só o wallpaper | | |
| 5.4 | Zip hostil: entrada `../x.txt`, `C:/x`, `a\b` junto de um `theme.toml` válido | "caminho perigoso"; nada importado | | |
| 5.5 | `theme.toml` com `[mods]` e `[shortcuts]` com linhas de comando e um `payload.exe` no zip | Importa; "Desligado: seção [mods]…"; o `payload.exe` é ignorado e nunca extraído; nada roda | | |
| 5.6 | Tema com um id de ajuste que exige administrador (`taskbar-widgets-off`) e outro inexistente | Os dois ficam desligados com o motivo | | |
| 5.7 | Arquivo com mais de 1 MB, ou `theme.toml` de 100 KB, ou não-zip renomeado | Recusado com mensagem clara; o Sentinela continua normal | | |
| 6 | **Dois monitores e playlists** | | | |
| 6.1 | Dois monitores com wallpapers diferentes; **Usar** um wallpaper | Vale para os dois (o `-monitor` por monitor ainda não tem botão); anotar se atrapalha | | |
| 6.2 | Wallpaper Engine com playlist ativa; aplicar um tema com wallpaper | A playlist é substituída; Desfazer volta ao `file` anterior (anotar se voltar a playlist inteira é necessário) | | |
| 6.3 | Trocar o wallpaper pelo próprio Wallpaper Engine e aplicar "Translúcido" (cor "do wallpaper") | A cor sai do wallpaper em uso agora (lido do `config.json`); se o `config.json` tiver outro formato, o aviso diz que a cor foi mantida | | |
