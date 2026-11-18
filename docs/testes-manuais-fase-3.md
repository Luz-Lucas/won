# Roteiro de testes manuais — Fase 3 (aplicativos)

Execute em uma **VM Windows 11 com snapshot** (restaure o snapshot antes de cada rodada), com o winget (Instalador de Aplicativo) 1.6 ou mais novo e acesso à internet. Use a conta de um usuário comum com permissão de elevar via UAC. Marque o resultado de cada item: **OK**, **FALHA** (descreva) ou **N/A**.

> Estado desta cópia: roteiro escrito, **ainda não executado**. A coluna Resultado está vazia.

| # | Passo | Esperado | Resultado | Observações |
| --- | --- | --- | --- | --- |
| 1 | **Instalar dois aplicativos leves pela janela** | | | |
| 1.1 | Abrir o Sentinela e entrar em Aplicativos | A lista do catálogo aparece, agrupável por categoria; 7-Zip e Everything aparecem sem selo (não instalados) | | |
| 1.2 | Marcar 7-Zip e Everything e clicar em Instalar | Diálogo `Instalar 2 aplicativo(s)?` com o texto "Ao continuar, você aceita os termos de licença de cada aplicativo."; nada é instalado antes de confirmar | | |
| 1.3 | Confirmar e acompanhar a fila | Um aplicativo por vez, com progresso por item (`[1/2]`, `[2/2]`); ao final, resultado legível por item | | |
| 1.4 | Conferir os selos | 7-Zip e Everything aparecem com o selo "Instalado"; os dois existem no menu Iniciar | | |
| 2 | **Atualizar tudo** | | | |
| 2.1 | Com algum aplicativo desatualizado (ou instale uma versão antiga pelo winget com `--version`), reabrir Aplicativos | O aplicativo mostra o selo "Atualização disponível" | | |
| 2.2 | Clicar em "Atualizar tudo" e confirmar | Só os aplicativos com atualização disponível entram na fila; o botão fica desabilitado durante o job | | |
| 2.3 | Ao terminar | Selo passa a "Instalado"; sem atualizações o botão "Atualizar tudo" fica desabilitado | | |
| 3 | **Recusar o UAC de um instalador** | | | |
| 3.1 | Instalar um aplicativo cujo instalador pede elevação (por exemplo, um jogo/launcher) e clicar em **Não** no UAC | O item falha com mensagem legível (sem código cru); a fila segue ou termina sem travar a janela | | |
| 3.2 | Conferir o selo do aplicativo recusado | Continua sem selo de instalado | | |
| 4 | **Parar a fila no meio** | | | |
| 4.1 | Selecionar 3 aplicativos e confirmar a instalação | Fila começa | | |
| 4.2 | Durante o primeiro item, clicar em "Parar depois deste" | O item em andamento **termina** (o winget não é morto); os demais não são processados e aparecem como não processados | | |
| 4.3 | Conferir o Gerenciador de Tarefas | Nenhum `winget.exe` ou instalador órfão rodando depois do fim da fila | | |
| 5 | **CLI no terminal** | | | |
| 5.1 | Rodar `sentinela apps list` | Uma linha por aplicativo: id, nome, categoria e estado (`instalado`, `atualização disponível`, `não instalado`) | | |
| 5.2 | Rodar `sentinela apps list --category navegadores --json` | Lista JSON só com a categoria; `state` em `installed`, `update_available` ou `not_installed` | | |
| 5.3 | Rodar `sentinela apps install 7zip` e responder `n` | Mostra o aplicativo e o texto de licença; cancela com "Cancelado: nada foi alterado."; nada é instalado | | |
| 5.4 | Rodar `sentinela apps install 7zip everything --yes` | Linhas `[1/2] 7-Zip… instalado.` e `[2/2] Everything… instalado.` (ou "já instalado"); código de saída 0 | | |
| 5.5 | Rodar `sentinela apps upgrade --all` quando nada estiver desatualizado | `Nada para atualizar.` e código 0 | | |
| 5.6 | Rodar `sentinela apps install nao-existe` | Erro `aplicativo desconhecido: nao-existe. Use `sentinela apps list`.`, código 2, sem pergunta | | |
| 5.7 | Durante uma fila no terminal, pressionar Ctrl+C | O item atual termina; imprime "Interrompido: N aplicativo(s) não processado(s)."; código 1 | | |
| 6 | **Desinstalar pelo Windows e conferir o selo** | | | |
| 6.1 | Desinstalar o 7-Zip em Configurações > Aplicativos | Desinstalação normal | | |
| 6.2 | Na página Aplicativos, clicar em "Tentar de novo" (ou fechar e reabrir o Sentinela) | O selo "Instalado" do 7-Zip some | | |
| 6.3 | Rodar `sentinela apps list` | 7-Zip aparece como `não instalado` | | |
