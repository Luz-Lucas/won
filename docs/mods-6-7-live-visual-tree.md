# Roteiro para o Lucas: coletar o que falta dos mods 6 e 7 com o Live Visual Tree

Isto não precisa de mim rodando nada: é uma ferramenta gráfica do Visual Studio, interativa, que só
funciona com você olhando a tela. Serve para achar o que o PDB não dá: o objeto de verdade que está
desenhando a barra agora, e qual propriedade dele muda o estilo.

## 1. Preparar
1. Instale o **Visual Studio 2022** (Community serve), com a carga "Desenvolvimento para desktop com
   C++" (precisa das ferramentas de depuração nativas).
2. Feche o instalador. Não precisa abrir nenhum projeto.

## 2. Anexar no Explorer vivo
1. Abra o Visual Studio sem projeto nenhum (`Arquivo > Abrir` cancelado, ou a tela inicial).
2. Menu `Depurar > Anexar ao Processo...` (Debug > Attach to Process).
3. Tipo de código de conexão: deixe "Automático"; na lista, selecione **explorer.exe**.
4. Clique em Anexar. A primeira vez pode pedir para habilitar "Ferramentas de Diagnóstico XAML" —
   aceite. Isso não reinicia o Explorer; pior caso realista é a barra piscar.

## 3. Abrir a árvore ao vivo
1. Menu `Depurar > Windows > Live Visual Tree` (ou `Ctrl+Alt+K` dependendo do idioma do menu).
2. Na árvore, clique no ícone de mira ("Selecionar elemento na tela") e clique na barra de tarefas.
   Isso leva direto ao nó `TaskbarBackground` (ou `TaskbarFrame`) que está desenhando aquela barra.
3. Abra também o painel **Live Property Explorer** (no mesmo menu `Depurar > Windows`) com o nó
   selecionado: ele mostra `TaskbarTheme`, `IsBorderVisible` etc. com o valor atual.
4. Teste mudando o valor de uma propriedade ali mesmo (o painel permite editar ao vivo). Se a barra
   mudar na hora, essa é a propriedade certa para o mod 6; anote o nome exato que apareceu no painel
   (pode ter mudado de nome entre builds do Windows).
5. Repita abrindo o menu Iniciar para achar o nó equivalente do mod 7 (procure por algo como
   `StartMenu` ou `LauncherFrame` na árvore).

## 4. O que me trazer
Para cada mod, me diga:
- o nome exato da classe do nó na árvore (ex.: `Taskbar.TaskbarBackground`);
- o nome da propriedade que mudou o visual ao vivo;
- um print da árvore, se puder.

Com isso eu escrevo o código que acha essa instância em tempo real dentro do mod (usando a mesma API
de automação que o Live Visual Tree usa por baixo) e troca a propriedade pelo valor que você escolher,
em vez de chutar um ponteiro. Isso fecha a fase 13 com segurança.

Veja também [docs/fases-13-14-o-que-falta.md](fases-13-14-o-que-falta.md) para o porquê disso ser
necessário, e o PR com os mods 4 e 5 já prontos: https://github.com/Luz-Lucas/won/pull/20
