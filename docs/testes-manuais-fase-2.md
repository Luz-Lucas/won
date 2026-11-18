# Roteiro de testes manuais — Fase 2 (otimizações)

Execute em uma **VM Windows 11 com snapshot** (restaure o snapshot antes de cada rodada). Use a conta de um usuário comum (não administrador) com permissão de elevar via UAC. Marque o resultado de cada item: **OK**, **FALHA** (descreva) ou **N/A**.

> Estado desta cópia: roteiro escrito, **ainda não executado**. A coluna Resultado está vazia.

| # | Passo | Esperado | Resultado | Observações |
| --- | --- | --- | --- | --- |
| 1 | **Perfil Recomendado sem administrador (só HKCU)** | | | |
| 1.1 | Abrir o Sentinela sem elevar e entrar em Jogos | Perfil "Recomendado" selecionado; otimizações que exigem administrador aparecem com o selo "Requer administrador" e o aviso "Algumas otimizações exigem administrador." | | |
| 1.2 | Conferir a Prévia e clicar em Aplicar; confirmar | Diálogo de confirmação aparece; nada muda antes de confirmar | | |
| 1.3 | Ao terminar, ler o relatório | Só as otimizações de HKCU foram aplicadas; nenhuma pede UAC; sem ponto de restauração criado | | |
| 1.4 | Conferir no `regedit` (HKCU) | Valores escritos conforme a Prévia | | |
| 2 | **Elevar e aplicar o resto** (o UAC pede a senha de **outra** conta, a de administrador) | | | |
| 2.1 | Clicar em "Reiniciar como administrador" e entrar com a conta de administrador no UAC | Nova janela elevada; o aviso de administrador some | | |
| 2.2 | Na janela elevada, abrir Jogos e Histórico | As duas páginas mostram "Esta janela está em outra conta do Windows. O histórico e os ajustes do usuário mostrados aqui são dessa conta."; as otimizações de HKCU aparecem bloqueadas; o Histórico **não** mostra o que foi aplicado no passo 1 (histórico separado por conta) | | |
| 2.3 | Em Jogos, aplicar o perfil Recomendado e confirmar | O diálogo avisa "Um ponto de restauração do Windows será solicitado antes." | | |
| 2.4 | Ao terminar, ler o relatório | Só as otimizações de HKLM/serviços foram aplicadas; o relatório mostra o ponto de restauração; em "Propriedades do Sistema > Proteção do Sistema > Restauração do Sistema" ele existe | | |
| 2.5 | Aplicar de novo algo que escreve, na mesma janela | O diálogo **não** fala mais em ponto de restauração (já solicitado nesta sessão) | | |
| 2.6 | Voltar à janela normal (sem elevar) e abrir o Histórico | Sem o aviso de outra conta; lista só o que foi aplicado no passo 1; se houver entradas de HKLM/serviços aparece "Algumas otimizações exigem administrador." com "Reiniciar como administrador" | | |
| 2.7 | Variante: conta já administradora (o UAC só pede confirmação) | Na janela elevada, sem o aviso de outra conta; o Histórico mostra tudo, de HKCU e de HKLM, num só histórico | | |
| 3 | **Reiniciar e conferir estados** | | | |
| 3.1 | Reiniciar o Windows (se o relatório pediu) e abrir o Sentinela | Em Jogos, as otimizações aplicadas aparecem como "Aplicado" | | |
| 3.2 | Abrir o Início | O cartão "Otimização" mostra `{aplicadas} de {total}` coerente com Jogos; o botão "Ir para Jogos" abre a página; a versão aparece no rodapé como "Sentinela {versão}" | | |
| 4 | **Alterar valor no `regedit` e desfazer (conflito)** | | | |
| 4.1 | No `regedit`, alterar manualmente o valor de uma chave escrita por uma otimização | Valor alterado | | |
| 4.2 | Em Histórico, clicar em "Desfazer" na linha dessa otimização | Confirmação `Desfazer "{título}"?`; nada muda antes de confirmar | | |
| 4.3 | Confirmar e ler o resultado | O valor alterado à mão é mantido e listado sob "Mantido porque mudou depois da otimização:"; os demais valores voltam ao original | | |
| 5 | **Desfazer tudo** (fazer nas duas janelas: as entradas de HKLM/serviços na janela elevada, as de HKCU na janela normal) | | | |
| 5.1 | Em Histórico, clicar em "Desfazer tudo" | Confirmação "Desfazer todas as {n} otimizações?" | | |
| 5.2 | Confirmar | Resultado por otimização; durante o job todos os botões "Desfazer" ficam desabilitados; ao final aparece "Nenhuma otimização aplicada pelo Sentinela." | | |
| 5.3 | Conferir no `regedit` | Valores que não existiam antes da otimização foram removidos; os demais voltaram ao valor original (exceto conflitos já listados) | | |
| 5.4 | Voltar ao Início | O cartão "Otimização" mostra `0 de {total}` | | |
| 6 | **CLI no terminal** | | | |
| 6.1 | Rodar `sentinela tweak list` | Lista as otimizações com o estado de cada uma | | |
| 6.2 | Rodar `sentinela tweak apply <id>` | Aplica e imprime o resultado; sem administrador, as que exigem elevação são recusadas com mensagem clara | | |
| 6.3 | Rodar `sentinela tweak undo <id>` | Desfaz e reporta conflitos, se houver; `sentinela tweak list` reflete o estado | | |
| 6.4 | Com o Sentinela desfazendo tudo numa janela, rodar `sentinela tweak undo --all --yes` ao mesmo tempo | O segundo espera o primeiro terminar (até 10 s); se demorar mais, cada item falha com "falhou: outro processo está alterando as otimizações; tente de novo."; o histórico continua íntegro (nenhuma entrada some) | | |
