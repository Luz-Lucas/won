# Sentinela — Fase 9: lançador — Plano de implementação

**Spec:** §9.3 (peça "Lançador"), §11 fase 9. Depende da fase 6.

**Escopo:** caixa de busca por atalho global com apps do menu Iniciar, janelas abertas, apps do próprio `desktop.toml`, ações do Sentinela e calculadora. Fica para depois: busca de arquivos, IME/acentos compostos na digitação, ícones, histórico de uso.

**Decisões:**
- `launcher.rs` é puro: pontuação (prefixo > início de palavra > trecho > letras em ordem), ordenação com limite e uma calculadora (`+ - * / % ^`, parênteses, vírgula decimal, `x`/`×`/`÷`). Número sozinho, divisão por zero e conta incompleta não viram resultado.
- Seção `[launcher]`: `hotkey` (obrigatória, entra na tabela de atalhos e conflita como os outros), `max_results` (3 a 15) e `[[launcher.app]]` com nome, comando e argumentos do próprio usuário.
- Apps: atalhos `.lnk` do menu Iniciar (do sistema e do usuário), sem os de desinstalação; índice refeito a cada 5 minutos. Abre com `ShellExecuteW`. Janelas abertas entram na hora, e escolher uma a restaura e traz para a frente.
- Teclas: digitar, Backspace, ↑/↓, Enter, Esc; perder o foco fecha. Em jogo (janela cobrindo o monitor) o lançador não abre.
- Ações do Sentinela: "Abrir o Sentinela" e "Encerrar o sentinela-desktop" (desfaz o que mudou nas janelas, como a saída de emergência).

**Tarefas:** 1) `launcher.rs` (testes). 2) `[launcher]` no `config.rs`. 3) `winlauncher.rs` e integração. 4) README e `docs/testes-manuais-fase-9.md`.
