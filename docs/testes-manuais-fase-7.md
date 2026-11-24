# Roteiro de testes manuais — Fase 7 (barra própria)

Execute num **PC com Windows 11**, com a fase 6 funcionando. Para o passo 6, um segundo monitor; para o 5, um jogo em tela cheia. Marque **OK**, **FALHA** (descreva) ou **N/A**.

> Estado desta cópia: roteiro escrito, **ainda não executado**. Fora do Windows foram conferidos só a configuração `[bar]`, o formato do relógio, a geometria e o cálculo de CPU, mais a compilação para Windows. **Nada que desenha a barra ou fala com o Explorer foi executado.**

| # | Passo | Esperado | Resultado | Observações |
| --- | --- | --- | --- | --- |
| 1 | **Barra** | | | |
| 1.1 | Descomentar `[bar]` no `desktop.toml` e salvar com o programa rodando | A barra aparece no topo do monitor principal, sem roubar o foco | | |
| 1.2 | Maximizar uma janela | Ela fica abaixo da barra, sem ficar escondida atrás | | |
| 1.3 | `edge = "bottom"` e `height = 48` | A barra vai para o rodapé com a nova altura; a área útil se ajusta | | |
| 1.4 | Cores, fonte e `font_size` | Mudam na hora; texto legível | | |
| 2 | **Módulos** | | | |
| 2.1 | `title` | Mostra o título da janela em foco e acompanha ao trocar de janela; títulos longos terminam em "…" | | |
| 2.2 | `clock` com `clock_format = "%d/%m %H:%M:%S"` | Hora certa, atualizando a cada segundo | | |
| 2.3 | `cpu` e `ram` | Valores plausíveis comparados com o Gerenciador de Tarefas | | |
| 2.4 | `center` com módulos | Centralizado sem encostar nos outros grupos | | |
| 3 | **Barra do Windows** | | | |
| 3.1 | `hide_native_taskbar = true` | A barra do Windows some; a nossa continua | | |
| 3.2 | Parar pela janela | A barra do Windows volta, a nossa some, a área útil volta ao normal | | |
| 3.3 | Matar o `sentinela-desktop.exe` pelo Gerenciador de Tarefas com a barra escondida; depois rodar `sentinela-desktop.exe --stop` | A barra do Windows volta | | |
| 3.4 | Reiniciar o Explorer com a barra escondida | A barra do Windows reaparece e some de novo em ~1 s | | |
| 3.5 | `Win+Ctrl+Shift+Esc` | Tudo volta: barra do Windows, janelas e processo encerrado | | |
| 4 | **Arquivo** | | | |
| 4.1 | `height = 5` ou módulo desconhecido | A janela recusa; arquivo salvo à mão com erro mantém a barra anterior | | |
| 4.2 | Remover `[bar]` | A barra some e a área útil volta | | |
| 5 | **Jogos** | | | |
| 5.1 | Jogo em tela cheia/sem borda | A barra se esconde enquanto o jogo está em foco e volta depois | | |
| 6 | **Monitores e DPI** | | | |
| 6.1 | Dois monitores | A barra fica só no principal (limitação desta fase) | | |
| 6.2 | Escala de 150% | Texto e altura proporcionais, sem cortes | | |
| 7 | **Convivência** | | | |
| 7.1 | Wallpaper Engine animado | Continua animando por trás; a barra não pisca | | |
| 7.2 | Recursos | Parado: CPU perto de 0% e poucos MB | | |
