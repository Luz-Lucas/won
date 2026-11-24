# Roteiro de testes manuais — Fase 6 (`sentinela-desktop`)

Execute num **PC com Windows 11**, com o Sentinela **instalado** (o instalador traz o `sentinela-desktop.exe`). Para o passo 7, o Wallpaper Engine; para o 8, um jogo em tela cheia ou sem borda. Marque **OK**, **FALHA** (descreva) ou **N/A**.

> Estado desta cópia: roteiro escrito, **ainda não executado**. Fora do Windows foram conferidos só a leitura e a validação do `desktop.toml`, a conversão de atalhos, o casamento de regras e a detecção de tela cheia, mais a compilação para Windows (`clippy` no alvo `x86_64-pc-windows-msvc`). **Nada que usa as APIs de janelas, atalhos e DWM foi executado.** Em especial, quais atributos do DWM cada programa aceita de fora só se sabe testando.

| # | Passo | Esperado | Resultado | Observações |
| --- | --- | --- | --- | --- |
| 1 | **Janela** | | | |
| 1.1 | Personalizar → **Desktop** → Iniciar | Passa a "Em execução"; um `sentinela-desktop.exe` no Gerenciador de Tarefas, sem janela | | |
| 1.2 | Clicar Iniciar de novo / abrir o exe à mão | Continua um só processo | | |
| 1.3 | **Parar** | O processo some; tudo que ele mudou nas janelas volta ao normal | | |
| 2 | **Arquivo** | | | |
| 2.1 | Colar uma regra com `opacity = 5` | A janela mostra o problema e **não deixa salvar** | | |
| 2.2 | Salvar um arquivo válido com o programa rodando | Vale em até ~1 s, sem reiniciar | | |
| 2.3 | Editar `desktop.toml` à mão com erro de sintaxe e salvar | O programa **continua com a configuração anterior**; a janela mostra o erro | | |
| 2.4 | Conferir `desktop.toml.bak` | Tem o conteúdo anterior | | |
| 3 | **Atalhos** | | | |
| 3.1 | `Win+Alt+T` → `toggle-topmost` no Bloco de Notas | Fica sempre no topo; de novo desliga | | |
| 3.2 | `opacity-down` / `opacity-up` | A janela ativa fica mais/menos transparente em passos de 10%, entre 20% e 100% | | |
| 3.3 | `center`, `toggle-maximize` | Centraliza na área útil do monitor; alterna maximizar | | |
| 3.4 | Atalho `run` abrindo o Terminal | Abre, sem passar por shell (aspas e espaços nos `args` ficam como escritos) | | |
| 3.5 | Atalho já usado por outro programa | Esse atalho fica sem efeito; os outros funcionam | | |
| 4 | **Regras** | | | |
| 4.1 | Regra por `exe`: `opacity = 85`, `corners = "round"` | Vale em janelas abertas e novas | | |
| 4.2 | `backdrop = "mica"` / `"acrylic"` | Funciona em quais programas? Anotar os que **não** aceitam (esperado: alguns ignoram) | | |
| 4.3 | `no_titlebar = true` | A barra de título some; remover a regra a traz de volta | | |
| 4.4 | Regra `exe = "*"` | Não afeta a área de trabalho, a barra do Windows nem o Sentinela | | |
| 4.5 | Remover a regra e salvar | A janela volta ao que era | | |
| 5 | **Bordas** | | | |
| 5.1 | `[borders] active/inactive` | A janela ativa ganha a cor; ao trocar de janela, a anterior vai para a cor "inativa" | | |
| 5.2 | Remover a seção | As bordas voltam ao padrão | | |
| 6 | **Emergência** | | | |
| 6.1 | Com regras e bordas ativas, `Win+Ctrl+Shift+Esc` | Tudo volta ao normal e o processo encerra | | |
| 6.2 | Tentar usar essa combinação em `[[hotkey]]` | O arquivo é recusado com a explicação | | |
| 7 | **Wallpaper Engine** | | | |
| 7.1 | Regra `exe = "*"` com opacidade, wallpaper animado ligado | O wallpaper continua animando; nada pisca na área de trabalho | | |
| 8 | **Jogos** | | | |
| 8.1 | Jogo em tela cheia/sem borda com regra para o `.exe` dele | **Nada** muda na janela do jogo | | |
| 8.2 | No jogo, `toggle-topmost` | Sem efeito | | |
| 8.3 | Jogo com anti-cheat | Nenhum aviso; o `sentinela-desktop` não injeta nada | | |
| 9 | **Recursos** | | | |
| 9.1 | Deixar rodando parado | CPU perto de 0%; memória baixa (meta: ~30 MB) | | |
