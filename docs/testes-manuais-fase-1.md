# Roteiro de testes manuais — Fase 1 (janela)

Execute em uma **VM Windows 11 com snapshot** (restaure o snapshot antes de cada rodada). Use a conta de um usuário comum (não administrador) com permissão de elevar via UAC. Marque o resultado de cada item: **OK**, **FALHA** (descreva) ou **N/A**.

Instalador: `Sentinela_<versão>_x64-setup.exe`.

> Estado desta cópia: roteiro escrito, **ainda não executado**. As colunas de resultado estão vazias.

| # | Passo | Resultado esperado | Resultado | Obs. |
| --- | --- | --- | --- | --- |
| 1 | **Instalar** | | | |
| 1.1 | Executar o instalador (aceitar o UAC) e concluir a instalação | Instalação por máquina em `C:\Program Files\Sentinela`; atalho no Menu Iniciar | | |
| 1.2 | Conferir a pasta de instalação | `sentinela-gui.exe` **e** `sentinela.exe` lado a lado, mais o desinstalador | | |
| 1.3 | No terminal, rodar `"C:\Program Files\Sentinela\sentinela.exe" info` | Imprime a versão e "Executando como administrador: não" | | |
| 2 | **Abrir sem administrador** | | | |
| 2.1 | Abrir o Sentinela pelo Menu Iniciar (sem elevar) | A janela abre, sem pedir UAC; título "Sentinela" | | |
| 3 | **Cada página (verificar visualmente)** | | | |
| 3.1 | Percorrer Início, Limpeza, Segurança, Disco e Ajustes | Nenhuma página em branco; textos em português; sem sobreposição ou corte em 1100x720 nem na janela mínima 900x600; nenhum erro visível | | |
| 3.2 | Redimensionar até o mínimo e maximizar | Layout se adapta; sem barras de rolagem indevidas | | |
| 4 | **UAC: recusar e aceitar** | | | |
| 4.1 | Clicar em "Reiniciar como administrador" no rodapé da barra lateral e **recusar** o UAC | A janela continua aberta, sem travar; mensagem informa que a elevação foi cancelada | | |
| 4.2 | Repetir e **aceitar** o UAC | A janela atual fecha e uma nova abre elevada (a antiga só sai depois de a nova subir, via `--wait-pid`); fica **uma** única janela; o rodapé da barra lateral mostra "Executando como administrador: sim" e o botão "Reiniciar como administrador" desaparece | | |
| 4.3 | Conferir no Gerenciador de Tarefas | Apenas um processo `sentinela-gui.exe` (elevado) | | |
| 5 | **Limpeza real em `%TEMP%`** | | | |
| 5.1 | Criar em `%TEMP%` alguns arquivos de teste com mais de 24 h (ex.: `fsutil file createnew` e ajustar a data de modificação e de criação para 2 dias atrás) e outros recentes | | | |
| 5.2 | Na página Limpeza, aguardar a simulação inicial | Lista as categorias com arquivos e tamanhos; **nada é apagado** antes de confirmar (conferir no Explorer) | | |
| 5.3 | Desmarcar a categoria `user-temp` e aplicar | Os arquivos de teste de `%TEMP%` continuam lá (categorias desmarcadas não são apagadas) | | |
| 5.4 | Marcar `user-temp` e confirmar | Só os arquivos antigos somem; os recentes permanecem; o relatório mostra o espaço liberado | | |
| 5.5 | Usar "Simular novamente" | Nova simulação, sem apagar nada, refletindo o estado atual | | |
| 6 | **Verificação de pasta com EICAR** | | | |
| 6.1 | Escolher uma pasta que o Defender não vigia/excluiu de forma natural (por exemplo, uma pasta em uma unidade/VHD secundário). **Não crie exclusões no Defender.** Se o Defender remover o arquivo de teste, anote e use o item 6.3 | | | |
| 6.2 | Criar `eicar.com` com a string padrão EICAR (68 caracteres) e verificar a pasta | Achado `[MALICIOSO]` com a regra EICAR; o arquivo **não** é apagado nem movido pela varredura | | |
| 6.3 | Se o Defender barrou a criação do arquivo, verificar a pasta mesmo assim | Verificação termina sem erro; resultado "nada encontrado" | | |
| 7 | **Cancelar uma verificação completa** | | | |
| 7.1 | Iniciar a verificação completa e clicar em Cancelar | Para em poucos segundos; mostra relatório parcial; a limpeza automática **não** roda | | |
| 8 | **Fechar a janela durante uma verificação** | | | |
| 8.1 | Iniciar uma verificação e fechar a janela no X | A janela fecha | | |
| 8.2 | Abrir o Gerenciador de Tarefas | O processo `sentinela-gui.exe` (e nenhum filho) saiu; nada fica rodando | | |
| 9 | **Quarentena criada pelo CLI** | | | |
| 9.1 | Criar o arquivo EICAR e rodar `sentinela.exe full` (ou usar um item de quarentena existente) para isolá-lo | `sentinela.exe quarantine` lista o item | | |
| 9.2 | Em Segurança, na seção Quarentena, restaurar o item | Arquivo volta ao caminho original; se o caminho existir, a restauração recusa sobrescrever e mantém o item | | |
| 10 | **Atualizar assinaturas** | | | |
| 10.1 | Em Ajustes, sem chave da abuse.ch salva: atualizar o banco | Só as regras YARA são atualizadas, com aviso sobre a chave; chave nunca aparece na tela | | |
| 10.2 | Salvar a chave em Ajustes e atualizar o banco | Hashes e regras atualizados; a chave nunca é exibida | | |
| 11 | **Abrir o app duas vezes** | | | |
| 11.1 | Com o Sentinela aberto, abra-o novamente pelo Menu Iniciar | Nenhuma segunda janela: a existente vem para a frente | | |
| 12 | **Desinstalar** | | | |
| 12.1 | Desinstalar pelo Menu Iniciar ou "Aplicativos instalados" | Pasta de instalação e atalhos removidos | | |
| 12.2 | Conferir `%LOCALAPPDATA%\Sentinela` | Quarentena e banco permanecem, salvo se a opção de apagar dados foi marcada | | |

## Registro

- Versão testada:
- Data e testador:
- Observações gerais:
