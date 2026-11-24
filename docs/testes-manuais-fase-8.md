# Roteiro de testes manuais — Fase 8 (tiling)

Execute num **PC com Windows 11**, com as fases 6 e 7 funcionando. Para o passo 6, dois monitores. Marque **OK**, **FALHA** (descreva) ou **N/A**.

> Estado desta cópia: roteiro escrito, **ainda não executado**. Fora do Windows foram conferidos só os layouts (dentro da área, sem sobreposição, 1 a 12 janelas), a configuração `[tiling]` e as ações; mais a compilação para Windows. **Nada que move janelas reais foi executado.** Em especial, a compensação das bordas invisíveis e o foco por teclado (`SetForegroundWindow`) dependem do Windows.

| # | Passo | Esperado | Resultado | Observações |
| --- | --- | --- | --- | --- |
| 1 | **Ligar** | | | |
| 1.1 | Abrir 3 janelas (Bloco de Notas, Calculadora, Explorador); adicionar `[tiling]` e salvar | As janelas se organizam sem sobreposição, com espaços iguais | | |
| 1.2 | `tile-toggle` pelo atalho | Desliga: cada janela volta ao lugar de antes; liga de novo organiza | | |
| 1.3 | Remover `[tiling]` | Idem desligar | | |
| 2 | **Layouts** | | | |
| 2.1 | `tile-next-layout` repetidas vezes | master-stack → columns → grid → monocle → volta | | |
| 2.2 | `master-grow` / `master-shrink` | A mestra cresce/encolhe em passos de 5%, entre 20% e 80% | | |
| 2.3 | Com a barra da fase 7 ligada | As janelas respeitam a área abaixo da barra | | |
| 3 | **Teclado** | | | |
| 3.1 | `focus-next` / `focus-prev` | O foco anda entre as janelas organizadas, com volta | | |
| 3.2 | `swap-master` | A janela em foco vira a mestra; de novo troca com a segunda | | |
| 4 | **Flutuantes** | | | |
| 4.1 | `toggle-float` numa janela | Ela volta ao lugar de antes e as outras se reorganizam; de novo, volta ao tiling | | |
| 4.2 | Regra `float = true` para um exe | Janelas dele nunca entram no tiling | | |
| 4.3 | Diálogo "Salvar como", instalador, janela de tamanho fixo | Flutuam sozinhos | | |
| 5 | **Eventos** | | | |
| 5.1 | Abrir, fechar, minimizar e restaurar janelas | A disposição se ajusta em até ~1 s | | |
| 5.2 | Janela maximizada | Ao entrar no tiling é restaurada e encaixada | | |
| 5.3 | Redimensionar uma janela à mão | Fica como está até a próxima mudança no conjunto | | |
| 5.4 | Apps UWP (Configurações, Loja) | Entram ou flutuam sem ocupar espaço vazio (janelas "fantasma" do DWM não contam) | | |
| 6 | **Monitores** | | | |
| 6.1 | Dois monitores | Cada um organiza as próprias janelas na própria área útil | | |
| 6.2 | Arrastar janela para o outro monitor | Reorganiza os dois | | |
| 7 | **Jogos e emergência** | | | |
| 7.1 | Jogo em tela cheia aberto | Nada é movido enquanto ele está em foco | | |
| 7.2 | `Win+Ctrl+Shift+Esc` | Todas as janelas voltam ao lugar de antes e o programa encerra | | |
| 7.3 | Wallpaper Engine animado | Continua por trás sem piscar | | |
