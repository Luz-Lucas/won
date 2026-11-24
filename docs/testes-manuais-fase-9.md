# Roteiro de testes manuais — Fase 9 (lançador)

Execute num **PC com Windows 11**, com a fase 6 funcionando e `[launcher] hotkey = "Alt+Space"` no `desktop.toml`. Marque **OK**, **FALHA** (descreva) ou **N/A**.

> Estado desta cópia: roteiro escrito, **ainda não executado**. Fora do Windows foram conferidos só a busca, a calculadora e a configuração, mais a compilação para Windows. **Nada que desenha a caixa, lê o menu Iniciar ou traz janelas para a frente foi executado.** Em especial, se a caixa **recebe o foco** ao abrir (passo 1.2) depende das regras do Windows para quem pode roubar o foco.

| # | Passo | Esperado | Resultado | Observações |
| --- | --- | --- | --- | --- |
| 1 | **Abrir e fechar** | | | |
| 1.1 | `Alt+Space` | A caixa aparece no alto da tela, centralizada | | |
| 1.2 | Digitar logo em seguida | O texto aparece (a caixa recebeu o foco sem precisar clicar) | | |
| 1.3 | `Esc`, clicar fora, ou o atalho de novo | Fecha | | |
| 1.4 | Jogo em tela cheia, atalho | Não abre | | |
| 2 | **Busca** | | | |
| 2.1 | Digitar parte do nome de um app | Lista os apps do menu Iniciar, os melhores primeiro; sem "desinstalar" | | |
| 2.2 | ↑ / ↓ e Enter | O item marcado abre | | |
| 2.3 | `vsc` para "Visual Studio Code" | Acha por letras em ordem | | |
| 2.4 | Instalar um app novo e abrir o lançador depois de 5 min | Ele aparece | | |
| 3 | **Janelas** | | | |
| 3.1 | Digitar parte do título de uma janela aberta | Aparece com "janela" na frente; Enter a traz para a frente | | |
| 3.2 | Janela minimizada | É restaurada | | |
| 4 | **Calculadora** | | | |
| 4.1 | `2*(3+4)` | Mostra `= 14` | | |
| 4.2 | `1,5^2`, `10/4`, `-3+5` | `= 2,25`, `= 2,5`, `= 2` | | |
| 4.3 | `1/0`, `2+`, `42` | Sem resultado de conta | | |
| 5 | **Seus apps e ações** | | | |
| 5.1 | `[[launcher.app]]` com nome e comando | Aparece primeiro na lista e abre | | |
| 5.2 | "Abrir o Sentinela" | Abre a janela do Sentinela | | |
| 5.3 | "Encerrar o sentinela-desktop" | O programa encerra e desfaz o que mudou nas janelas | | |
| 6 | **Arquivo** | | | |
| 6.1 | `[launcher]` sem `hotkey`, ou com atalho repetido | A janela recusa com a explicação | | |
| 7 | **Limitações conhecidas desta fase** | | | |
| 7.1 | Acentos compostos (`´` + `a`) e IME | Pode não funcionar; anotar | | |
| 7.2 | Monitor com escala de 150% | Caixa e texto proporcionais | | |
