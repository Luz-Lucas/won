# Roteiro de testes manuais — Fase S2 (antivírus, parte 2)

Execute num **PC com Windows 11** com o Sentinela **instalado** (o menu do Explorador e a vigia de jogos só valem na versão instalada), com o banco de assinaturas baixado. Para os testes com arquivo detectado, use o **EICAR** ou o marcador inofensivo `SENTINELA-HARMLESS-TEST-MARKER` num arquivo de texto. Marque o resultado de cada item: **OK**, **FALHA** (descreva) ou **N/A**.

> Estado desta cópia: roteiro escrito, **ainda não executado**. Fora do Windows foram conferidos só a lógica pura, os limites de zip, o cache, a pausa e a compilação para Windows (`clippy` com o alvo `x86_64-pc-windows-msvc`). **Nada que fala com o Windows foi executado:** `WinVerifyTrust` e catálogos, registro do menu, prioridade de segundo plano, vigia de jogos e a janela. Os testes `#[ignore]` rodam no Windows com `cargo test -p sentinela-core -- --ignored real_system_files_are_signed writes_and_removes_real_keys`.

| # | Passo | Esperado | Resultado | Observações |
| --- | --- | --- | --- | --- |
| 1 | **Authenticode** | | | |
| 1.1 | `cargo test -p sentinela-core -- --ignored real_system_files_are_signed` | `notepad.exe` (embutida) e `kernel32.dll` (catálogo) com editor "Microsoft …"; arquivo falso sem assinatura | | |
| 1.2 | Verificação personalizada de `C:\Windows\System32` (como administrador) | Nenhum achado Suspeito por heurística em arquivos da Microsoft | | |
| 1.3 | Instalador conhecido, assinado por terceiros, que antes dava Suspeito (empacotado/entropia alta) | Deixa de ser Suspeito ou o detalhe mostra "valid Authenticode signature (…)" | | |
| 1.4 | Copiar um `.exe` assinado e alterar um byte no fim, verificar | Se der achado, o detalhe diz "invalid Authenticode signature" | | |
| 1.5 | Verificar com EICAR dentro de um `.exe` assinado (anexar o EICAR ao fim de um assinado) | Continua Malicioso: a assinatura não anula hash nem regra | | |
| 1.6 | Quarentena de um achado assinado pela Microsoft | Diálogo "Arquivos assinados pela Microsoft" com o nome do editor; Cancelar não move | | |
| 2 | **Nível Risco** | | | |
| 2.1 | Criar `foto.jpg.exe` (qualquer `.exe`) em Downloads e verificar | Grupo "Risco" com o motivo "extensão dupla"; desmarcado; sem Quarentena recomendada | | |
| 2.2 | Renomear um `.exe` para `contrato.pdf` | Risco: "disfarçado" (extensão de documento, por dentro é programa) | | |
| 2.3 | Pôr um `.vbs` na pasta Inicializar (`shell:startup`) | Risco: script na pasta Inicializar | | |
| 2.4 | Criar um atalho `.lnk` para `powershell.exe -enc SQBFAFgA` | Risco: PowerShell com comando codificado; um atalho com `-NoProfile` só não dá achado | | |
| 2.5 | Copiar um `.exe` sem assinatura para `%LOCALAPPDATA%\Teste` e registrar em `HKCU\...\Run`; rodar a Rápida | Risco: sem assinatura válida e inicia sozinho (Temp/AppData); o mesmo `.exe` fora do `Run` não dá achado | | |
| 2.6 | `.docm` com macro `Sub Document_Open()` (feito no Word) e um `.xlsm` com macro comum | Só o `Document_Open` vira Risco | | |
| 2.7 | `sentinela scan <pasta com os itens>` e `echo %ERRORLEVEL%` | Linhas `[RISCO]`; "Risco: N" no resumo; código 0 se só Risco | | |
| 3 | **Compactados** | | | |
| 3.1 | `.zip` com o marcador dentro de uma pasta | Achado no `.zip` com "dentro do compactado: pasta/arquivo"; diálogo "Achado dentro de um compactado" antes da quarentena; o `.zip` inteiro vai para a quarentena | | |
| 3.2 | `.zip` dentro de `.zip` dentro de `.zip` com o marcador no fundo | Dois níveis são verificados; o terceiro aparece em "Não verificados" | | |
| 3.3 | Bomba: `.zip` com 1 GB de zeros (`fsutil file createnew` e compactar) | Não consome memória sem limite; aparece em "Não verificados" com o motivo | | |
| 3.4 | `.zip` com senha | "protegido por senha" em "Não verificados" | | |
| 4 | **Cache** | | | |
| 4.1 | Rápida duas vezes seguidas | A segunda é visivelmente mais rápida; `%LOCALAPPDATA%\Sentinela\scan-cache.bin` existe | | |
| 4.2 | Alterar um arquivo (acrescentar o marcador) e repetir | O arquivo alterado é relido e encontrado | | |
| 4.3 | Atualizar o banco (Ajustes) e repetir | Regras novas descartam o cache: a verificação volta a ler tudo | | |
| 4.4 | Apagar ou corromper o `scan-cache.bin` | A verificação roda normal (sem erro) e o recria | | |
| 5 | **Segundo plano e pausa** | | | |
| 5.1 | Completa; olhar o Gerenciador de Tarefas | Uso de CPU até metade dos núcleos; o PC continua responsivo | | |
| 5.2 | Pausar e Continuar | O progresso para e volta; a mensagem "Verificação pausada." aparece | | |
| 5.3 | Cancelar enquanto pausada | Termina com o relatório parcial, sem travar | | |
| 5.4 | Com uma verificação em andamento, abrir um jogo detectado (ver Jogos) | "Verificação pausada: jogo aberto"; ao fechar o jogo, continua sozinha | | |
| 5.5 | Pausar pelo botão e abrir/fechar o jogo | Continua pausada ao fechar o jogo, até clicar em Continuar | | |
| 6 | **Clique direito** | | | |
| 6.1 | Ajustes → "Verificar com o Sentinela" no clique direito (ligado por padrão no app instalado) | `reg query HKCU\Software\Classes\*\shell\SentinelaScan\command` mostra `"...sentinela-gui.exe" --scan "%1"`; o mesmo em `Directory` e `Drive` | | |
| 6.2 | Clique direito num arquivo → Mostrar mais opções → Verificar com o Sentinela, com o app fechado | A janela abre em Segurança e verifica só o arquivo | | |
| 6.3 | O mesmo numa pasta e num disco (`D:\`), com o app aberto | A janela existente vem para a frente e verifica o item (sem segunda instância) | | |
| 6.4 | Pasta com espaço e acentos no nome | Funciona | | |
| 6.5 | Desligar a opção | As três chaves `SentinelaScan` somem; o item some do menu | | |
| 6.6 | Versão de desenvolvimento ou elevada | A opção fica travada com a explicação | | |
| 7 | **Cobertura** | | | |
| 7.1 | Rápida sem administrador | Aviso "… de fora por acesso negado" com "Reiniciar como administrador" | | |
| 7.2 | Clicar no botão | Pede o UAC, o app reabre elevado e o aviso não aparece mais | | |
