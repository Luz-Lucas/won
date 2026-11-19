# Roteiro de testes manuais — Fase 4a (personalização, limpeza nova e inicialização)

Execute em uma **VM Windows 11 (24H2 ou mais novo) com snapshot** (restaure o snapshot antes de cada rodada), com a Steam e o Discord instalados e com pelo menos um programa na inicialização. Use a conta de um usuário comum com permissão de elevar via UAC. Marque o resultado de cada item: **OK**, **FALHA** (descreva) ou **N/A**.

> Estado desta cópia: roteiro escrito, **ainda não executado**. A coluna Resultado está vazia.

| # | Passo | Esperado | Resultado | Observações |
| --- | --- | --- | --- | --- |
| 1 | **Personalizar: interruptores e reinício do Explorer** | | | |
| 1.1 | Abrir o Sentinela e entrar em Personalizar | Barra lateral com "Personalizar" logo depois de Aplicativos; abas Barra de tarefas, Menu Iniciar, Explorador, Aparência e Sistema | | |
| 1.2 | Na aba Explorador, ligar "Mostrar extensões de arquivos" | Diálogo `Aplicar "Mostrar extensões de arquivos"?` com a prévia `…HideFileExt: 1 → 0`; nada muda antes de confirmar | | |
| 1.3 | Confirmar | O interruptor fica ligado; aparece `Reinicie o Explorer para ver a mudança.` e o botão "Reiniciar Explorer"; o Explorer **não** é reiniciado sozinho | | |
| 1.4 | Abrir uma janela de pasta e clicar em "Reiniciar Explorer" | Diálogo `Reiniciar o Explorer? As janelas de pastas abertas serão fechadas.`; ao confirmar, a barra de tarefas some por alguns segundos e volta **uma vez só** (sem dois Explorers no Gerenciador de Tarefas); a janela de pasta fecha; aparece "Explorer reiniciado." | | |
| 1.5 | Abrir uma pasta | As extensões aparecem | | |
| 1.6 | Repetir 1.2–1.4 em uma opção de cada aba: "Menu de contexto clássico" (Explorador), "Ocultar recomendações do Iniciar" (Menu Iniciar), "Segundos no relógio" (Barra de tarefas), "Desligar o encaixe de janelas" (Sistema) | Cada mudança aparece depois do reinício do Explorer (o menu clássico ao clicar com o botão direito num arquivo) | | |
| 1.7 | Desligar "Mostrar extensões de arquivos" | Diálogo `Desfazer "Mostrar extensões de arquivos"?`; ao confirmar e reiniciar o Explorer, as extensões somem de novo | | |
| 1.8 | Em Configurações do Windows, ligar à mão "Mostrar arquivos ocultos"; voltar a Personalizar → Explorador | "Mostrar arquivos ocultos" aparece ligado, **travado**, com `Configurado fora do Sentinela.` | | |
| 2 | **Trocar de opção numa lista e desfazer pelo Histórico** (revisão 1) | | | |
| 2.1 | Anotar o alinhamento atual da barra (padrão: centro). Em Barra de tarefas → "Alinhamento da barra de tarefas", escolher "Esquerda" e confirmar; reiniciar o Explorer | Diálogo `Alinhamento da barra de tarefas: Esquerda?`; a barra vai para a esquerda; o botão "Esquerda" fica marcado | | |
| 2.2 | Escolher "Centro", confirmar e reiniciar o Explorer | A barra volta ao centro; "Centro" marcado; no Histórico só aparece "Centro" (a opção "Esquerda" foi desfeita antes) | | |
| 2.3 | Escolher "Esquerda" de novo e, no Histórico, desfazer | A barra volta ao **valor original da máquina** anotado em 2.1, e não ao "Centro" da troca anterior; nenhuma opção fica marcada na lista | | |
| 2.4 | Repetir com "Tema dos aplicativos": Escuro → Claro → desfazer tudo pelo Histórico | O tema volta ao original da máquina | | |
| 3 | **Widgets no build atual** (revisão 2) | | | |
| 3.1 | Sem administrador, abrir Barra de tarefas | "Desligar os Widgets" travado com "Requer administrador"; aviso "Algumas personalizações exigem administrador." com "Reiniciar como administrador" | | |
| 3.2 | Reiniciar como administrador e ligar "Desligar os Widgets" | Os Widgets somem da barra (depois de reiniciar o Explorer ou sair e entrar); se o Windows bloquear a escrita, a falha aparece com motivo legível e o Histórico **não** fica com um item pela metade | | |
| 3.3 | Desfazer pelo Histórico | Os Widgets voltam | | |
| 4 | **Limpeza: categorias novas** (revisão 3) | | | |
| 4.1 | Abrir Limpeza | Aparecem "Cache de shaders", "Cache de launchers", "Cache de miniaturas", "Cache da Otimização de Entrega" e "Logs do Windows"; sem administrador, as duas últimas ficam travadas com "requer administrador" | | |
| 4.2 | Com a **Steam aberta**, clicar em "Simular novamente" | "Cache de launchers" mostra o aviso `Steam está aberto; cache ignorado`; nenhum arquivo de `Steam\htmlcache` na amostra | | |
| 4.3 | Fechar a Steam (inclusive na bandeja) e o Discord, simular de novo e limpar só "Cache de launchers" | Os arquivos de `Steam\htmlcache` e das pastas `Cache`, `Code Cache` e `GPUCache` do Discord são apagados; `Steam\userdata`, os jogos e o login do Discord continuam (abrir a Steam e o Discord: continuam logados, jogos instalados) | | |
| 4.4 | Limpar "Cache de miniaturas" com uma pasta de fotos aberta | Arquivos `thumbcache_*.db` em uso são pulados e contados; o Explorer continua funcionando e recria as miniaturas | | |
| 4.5 | Reiniciar como administrador e limpar "Cache da Otimização de Entrega" e "Logs do Windows" | Liberam espaço; o `CBS.log` em uso continua em `C:\Windows\Logs\CBS`; nada fora dessas pastas some | | |
| 4.6 | Rodar `sentinela clean` (sem `--apply`) | As cinco categorias novas aparecem na simulação; nada é apagado | | |
| 5 | **Inicialização: desligar, reiniciar e religar** | | | |
| 5.1 | Em Jogos, rolar até "Programas que iniciam com o Windows" | Lista com nome, origem, comando encurtado (passar o mouse mostra o completo) e interruptor; a entrada "Sentinela" (se ligada em Ajustes) aparece **sem** interruptor, com `Este é o Sentinela. Para desligar, use Ajustes → Inicialização.` | | |
| 5.2 | Desligar um programa do usuário (ex.: Discord) | Diálogo `Desativar "Discord" na inicialização? O programa continua instalado.`; ao confirmar, o interruptor desliga; o Início mostra a atividade `Inicialização: "Discord" desativado` | | |
| 5.3 | Abrir o Gerenciador de Tarefas → Aplicativos de inicialização | O Discord aparece como **Desabilitado**; a entrada continua listada | | |
| 5.4 | Reiniciar o Windows | O Discord **não** abre sozinho; o atalho e o programa continuam instalados | | |
| 5.5 | Religar o Discord no Sentinela (sem confirmação) e reiniciar o Windows | O Gerenciador de Tarefas mostra **Habilitado** e o Discord abre com o Windows | | |
| 5.6 | Desabilitar um programa pelo Gerenciador de Tarefas e voltar ao Sentinela | O Sentinela mostra o mesmo programa desligado (os dois leem o mesmo estado) | | |
| 6 | **Entrada apontando para arquivo inexistente** (revisão 4) | | | |
| 6.1 | Criar uma entrada falsa: `reg add HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v Sumido /d "C:\nao\existe.exe"` e reabrir a página Jogos | "Sumido" aparece com o selo "Arquivo não encontrado" e com interruptor | | |
| 6.2 | Desligar "Sumido" | Fica desligado; a entrada **continua** no registro (`reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v Sumido`) | | |
| 6.3 | Limpar: `reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v Sumido /f` | Entrada removida à mão (o Sentinela nunca apaga) | | |
| 7 | **Entrada do sistema sem administrador** (revisão 5) | | | |
| 7.1 | Sem administrador, conferir uma entrada de "Registro (todos os usuários)" (ex.: SecurityHealth) | Interruptor travado, selo "🔒 Bloqueado" e o motivo `Requer administrador: vale para todos os usuários.` | | |
| 7.2 | Reiniciar como administrador | A mesma entrada fica com interruptor ativo | | |
| 8 | **CLI no terminal** | | | |
| 8.1 | Rodar `sentinela startup list` | Duas linhas por entrada: id, estado e nome; depois a origem e o comando; marcas `(arquivo não encontrado)`, `(requer administrador)` e `(este é o Sentinela)` quando for o caso | | |
| 8.2 | Rodar `sentinela startup disable run_hkcu:Discord` e responder `n` | Pergunta `Desativar "Discord" na inicialização? O programa continua instalado. [s/N]`; cancela com "Cancelado: nada foi alterado." | | |
| 8.3 | Rodar `sentinela startup disable run_hkcu:Discord --yes` e depois `sentinela startup enable run_hkcu:Discord --yes` | `Discord: desativado.` e `Discord: ativado.`; o Gerenciador de Tarefas acompanha | | |
| 8.4 | Sem administrador, rodar `sentinela startup disable run_hklm:SecurityHealth --yes` | Erro com "requer administrador", código 2, nada muda | | |
| 8.5 | Rodar `sentinela startup disable nao-existe` | `entrada de inicialização desconhecida: nao-existe. Use `sentinela startup list`.`, código 2, sem pergunta | | |
| 8.6 | Rodar `sentinela tweak list` | As personalizações (`taskbar-align-left`, `explorer-show-extensions`, …) aparecem | | |
| 8.7 | Rodar `sentinela tweak apply taskbar-align-left --yes` e depois `sentinela tweak apply taskbar-align-center --yes` | Termina com "Reinicie o Explorer para concluir."; o histórico fica só com `taskbar-align-center`; `sentinela tweak undo --all --yes` volta ao alinhamento original | | |
