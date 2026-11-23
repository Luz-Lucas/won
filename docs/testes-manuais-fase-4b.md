# Roteiro de testes manuais — Fase 4b (aba Avançado, GPU por jogo, Atualizações do Windows)

Execute em uma **VM Windows 11 (24H2 ou mais novo) com snapshot** (restaure o snapshot antes de cada rodada), com acesso à internet, a Steam instalada com ao menos um jogo e, se possível, duas GPUs (ou um notebook com GPU integrada e dedicada para a parte de GPU por jogo). Use a conta de um usuário comum com permissão de elevar via UAC. Marque o resultado de cada item: **OK**, **FALHA** (descreva) ou **N/A**.

> Estado desta cópia: roteiro escrito, **ainda não executado**. A coluna Resultado está vazia.

| # | Passo | Esperado | Resultado | Observações |
| --- | --- | --- | --- | --- |
| 1 | **Aba Avançado e perfil FPS competitivo** | | | |
| 1.1 | Abrir Jogos e clicar na aba "Avançado" | Lista com MMCSS, timer global, serviços em modo manual, indexação da busca, otimizações de tela cheia e ACK imediato do TCP; cada um com o selo Situacional ou Controverso e a explicação inteira | | |
| 1.2 | Clicar no perfil "FPS competitivo" | A aba Avançado abre; os situacionais ficam marcados, os controversos (timer e tela cheia) não | | |
| 1.3 | Voltar ao perfil "Recomendado" | Nada da aba Avançado fica marcado | | |
| 1.4 | Como administrador, aplicar "Serviços extras em modo manual" e reiniciar | `services.msc`: SysMain, MapsBroker e StorSvc em "Manual"; nenhum desativado | | |
| 1.5 | Desfazer pelo Histórico e reiniciar | Os três voltam ao tipo de inicialização original | | |
| 1.6 | Aplicar "ACK imediato do TCP" | `reg query HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces /s /v TcpAckFrequency` mostra `0x1` só nas interfaces com endereço; desfazer remove os valores criados | | |
| 2 | **GPU por jogo** | | | |
| 2.1 | Na seção "GPU por jogo" da página Jogos | Jogos da Steam (e Epic/Riot, se instalados) com o executável certo (jogos Unreal: o `*-Win64-Shipping.exe`); em VM com uma GPU, o aviso "Esta máquina tem uma GPU; a preferência não muda nada." | | |
| 2.2 | Ligar "Alto desempenho" em um jogo | Diálogo com o caminho do `.exe` e `GpuPreference → 2`; ao confirmar, Configurações → Sistema → Tela → Gráficos mostra o jogo com "Alto desempenho" | | |
| 2.3 | Desligar o mesmo jogo | Volta a "Deixar o Windows decidir" (o valor some de `HKCU\Software\Microsoft\DirectX\UserGpuPreferences`) | | |
| 2.4 | Ligar de novo, desinstalar o jogo pela Steam e reabrir a página Jogos (revisão 4) | O jogo some da lista e aparece "GPU de alto desempenho: {jogo}" com "Jogo não encontrado" e o botão "Desfazer", que remove o valor | | |
| 2.5 | Em Configurações → Gráficos, pôr outro jogo em "Alto desempenho" à mão e reabrir a página | O interruptor desse jogo aparece ligado e travado com "Configurado fora do Sentinela." | | |
| 2.6 | Clicar em "Adicionar jogo…" e escolher um `.exe` qualquer | Entra na lista como "Adicionado por você"; "Remover da lista" some com ele (com a preferência desligada) | | |
| 3 | **Modos de atualização** (revisão 1) | | | |
| 3.1 | Sem administrador, abrir Ajustes → Atualizações do Windows | Mostra a versão instalada; os modos ficam travados com o aviso de administrador | | |
| 3.2 | Reiniciar como administrador e escolher "Só segurança" | Prévia com `TargetReleaseVersion`, `ProductVersion` = "Windows 11", `TargetReleaseVersionInfo` = versão atual e os adiamentos; ao aplicar, `gpedit`/`reg query HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate` mostra os valores, e Configurações → Windows Update mostra "Algumas configurações são gerenciadas pela organização" | | |
| 3.3 | Trocar para "Bloqueado" | Prévia com os serviços (`wuauserv`, `UsoSvc`, `BITS` → Desativado), as tarefas e o aviso sobre correções de segurança e a Microsoft Store; ao aplicar, as políticas do "Só segurança" **somem** (nada fica do modo anterior) e `services.msc` mostra os três serviços desativados | | |
| 3.4 | Abrir o Agendador de Tarefas → Microsoft → Windows → UpdateOrchestrator (revisão 3) | As tarefas que aceitaram estão desabilitadas; as protegidas (acesso negado) aparecem no resultado como "não alterado" com o motivo, e não entram no Histórico | | |
| 3.5 | Escolher "Padrão" | Prévia do que volta; ao aplicar, serviços, tarefas e políticas voltam **exatamente** ao estado de antes do 3.2 (comparar com o snapshot) | | |
| 3.6 | Escolher "Pausado até…" com uma data daqui a 10 dias | Configurações → Windows Update mostra "Atualizações pausadas até {data}"; escolher outra data e aplicar troca a data (o Histórico fica só com a nova) | | |
| 4 | **O Windows religa o Windows Update** (revisão 2) | | | |
| 4.1 | Com "Bloqueado", em `services.msc` pôr o `UsoSvc` em "Automático" à mão e reabrir Ajustes | A seção mostra "Parcial: o Windows desfez parte do modo Bloqueado" com `Serviço UsoSvc (inicialização): Automático`; nada é reaplicado sozinho | | |
| 4.2 | Clicar em "Aplicar de novo" | O `UsoSvc` volta a "Desativado"; "Padrão" depois ainda devolve o original (Automático) | | |
| 5 | **Defender com atualizações bloqueadas** (revisão 5) | | | |
| 5.1 | Com "Bloqueado" aplicado, deixar o Sentinela aberto (ou na bandeja) por alguns minutos | A atividade recente do Início mostra "Definições do Defender atualizadas"; Segurança do Windows → Proteção contra vírus e ameaças → Atualizações mostra a hora da atualização | | |
| 5.2 | Se o item 5.1 falhar com o `wuauserv` desativado | Anotar a mensagem de "Definições do Defender: …" na atividade; é o caso do plano B do §6.5 (deixar o `wuauserv` em Manual no Bloqueado) | | |
| 6 | **Início e "Atualizar agora"** | | | |
| 6.1 | Com "Bloqueado" ou "Pausado", abrir o Início | Aviso "Atualizações do Windows bloqueadas desde hoje." (ou "há N dias") com "Atualizar agora" | | |
| 6.2 | Clicar em "Atualizar agora" e confirmar | O modo volta ao Padrão, o Windows Update abre; o Início mostra "Atualizações liberadas temporariamente" com "Ir para Ajustes" | | |
| 6.3 | Em Ajustes, clicar em "Voltar a Bloqueado" | O modo Bloqueado volta inteiro | | |
| 6.4 | Com "Bloqueado", abrir Aplicativos e tentar instalar um pacote da Microsoft Store | Aviso no topo sobre a Store e instaladores com "Liberar temporariamente"; a instalação falha com mensagem legível | | |
| 7 | **CLI no terminal** | | | |
| 7.1 | Rodar `sentinela updates status` | Versão do Windows e o modo atual (e "Parcial" com o que voltou, se for o caso) | | |
| 7.2 | Sem administrador, rodar `sentinela updates mode security` | Erro "exige o terminal como administrador", código 2, nada muda | | |
| 7.3 | Como administrador, `sentinela updates mode paused --until {data}` e responder `n` | Prévia no terminal, "Cancelado: nada foi alterado." | | |
| 7.4 | `sentinela updates mode default --yes` | "Atualizações: Padrão (…)" e código 0 | | |
| 7.5 | `sentinela games list` e `sentinela games gpu on {id} --yes` | Lista com os ids `gpu-…`; o jogo passa a mostrar `GPU+`; `games gpu off {id} --yes` desfaz | | |
| 7.6 | `sentinela tweak list` | A aba Avançado aparece (`mmcss-responsiveness`, `nagle-off`…) | | |
