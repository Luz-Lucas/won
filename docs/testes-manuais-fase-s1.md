# Roteiro de testes manuais — Fase S1 (antivírus na janela)

Execute num **PC com Windows 11** com o Sentinela instalado, com o banco de assinaturas baixado (Ajustes → Banco de assinaturas). Para os testes com arquivo detectado, use o **arquivo de teste EICAR** (inofensivo; o Defender pode apagá-lo — se isso acontecer, crie uma exclusão temporária para uma pasta de teste no Defender e remova-a no fim). Marque o resultado de cada item: **OK**, **FALHA** (descreva) ou **N/A**.

> Estado desta cópia: roteiro escrito, **ainda não executado** na janela. Na máquina de desenvolvimento foram conferidos só a leitura real das áreas críticas, a verificação rápida pelo CLI e a criação e remoção da tarefa no Agendador.

| # | Passo | Esperado | Resultado | Observações |
| --- | --- | --- | --- | --- |
| 1 | **Verificação rápida** (Segurança → Rápida, sem administrador) | | | |
| 1.1 | Rodar a rápida | Barra de progresso; no fim, total de arquivos, maliciosos e suspeitos; "Não verificados" inclui `C:\Windows\System32\Tasks` com "só podem ser lidas como administrador" | | |
| 1.2 | Repetir como administrador | As tarefas agendadas deixam de aparecer em "Não verificados" | | |
| 1.3 | Pôr o EICAR em Downloads como `teste.txt` e outro como `teste.com`, rodar a rápida | Só o `.com` é encontrado (o `.txt` não é tipo de risco); com a Completa ou Personalizada, os dois | | |
| 1.4 | Conferir que a pasta `%LOCALAPPDATA%\Sentinela` não aparece em nenhum achado | Nenhuma regra do próprio banco aparece como achado | | |
| 2 | **Achados e ações** | | | |
| 2.1 | Achado do EICAR | Vem marcado só se for por hash; mostra o "por quê" em português; Detalhes mostra o SHA-256 | | |
| 2.2 | Consultar no VirusTotal | Abre o navegador em `virustotal.com/gui/file/<sha256>`; nenhum arquivo é enviado | | |
| 2.3 | Ignorar sempre | Pede confirmação; o achado some e "1 achado ignorado" aparece; nova verificação não o mostra; Ajustes → Exceções lista o arquivo; Remover faz voltar a aparecer | | |
| 2.4 | Colocar em quarentena (1) | O arquivo sai do lugar; aparece em Quarentena; "1 arquivo colocado em quarentena." | | |
| 2.5 | Se algum achado estiver dentro de `C:\Windows` (senão, N/A), marcá-lo e pedir a quarentena | Diálogo "Arquivos da pasta do Windows"; Cancelar não move nada | | |
| 2.6 | Abrir um `.exe` de teste detectado (por exemplo um script empacotado seu marcado como suspeito), marcar e pôr em quarentena | Diálogo "Programas abertos" lista o programa; "Encerrar e colocar em quarentena" fecha o programa e move; se não fechar, aparece "Em uso, continua no lugar" | | |
| 3 | **Personalizada** | | | |
| 3.1 | Personalizada… e escolher duas pastas (uma num pendrive) | As duas são verificadas; o pendrive também | | |
| 4 | **Quarentena** (página Quarentena) | | | |
| 4.1 | Abrir `%LOCALAPPDATA%\Sentinela\quarantine\files` | O arquivo guardado não começa com `MZ` nem é o EICAR legível (codificado) | | |
| 4.2 | Restaurar | Pede confirmação; o arquivo volta ao lugar com o mesmo conteúdo | | |
| 4.3 | Apagar | Pede confirmação dizendo que não tem volta; o item some e o arquivo some da pasta `files` | | |
| 4.4 | Item posto em quarentena por uma versão anterior do Sentinela (se houver) | Restaura normalmente | | |
| 5 | **Agendamento** | | | |
| 5.1 | Primeira verificação pela janela | Aparece "Agendar uma verificação rápida toda semana?"; "Agora não" não pergunta de novo | | |
| 5.2 | Ajustes → Verificação rápida toda semana | `schtasks /Query /TN "Sentinela - verificacao rapida semanal"` mostra a tarefa (domingo 12:00) | | |
| 5.3 | No Agendador de Tarefas, "Executar" a tarefa com o Sentinela fechado | O Sentinela roda escondido, mostra uma notificação com o resultado e fecha; a Atividade registra a verificação | | |
| 5.4 | Executar a tarefa com o Sentinela aberto | A verificação roda na janela aberta (barra em Segurança) e notifica; o app continua aberto | | |
| 5.5 | Ligar "Colocar malware confirmado por hash em quarentena automaticamente", deixar o EICAR em Downloads e executar a tarefa | A notificação diz que 1 foi posto em quarentena; aparece em Quarentena | | |
| 5.6 | Desligar a verificação semanal | A tarefa some do Agendador | | |
| 6 | **Assinaturas** | | | |
| 6.1 | Com o banco baixado há mais de 24 h, abrir o Sentinela | A atualização roda sozinha (Ajustes mostra a operação; a data em Início muda) | | |
| 6.2 | Com o banco nunca baixado | Nada é baixado sozinho | | |
| 7 | **CLI** | | | |
| 7.1 | `sentinela scan --quick` | Mesmo comportamento da rápida da janela, com o resumo no fim | | |
| 7.2 | `sentinela quarantine --delete <id>` | Apaga o item; id inexistente encerra com código 2 | | |
