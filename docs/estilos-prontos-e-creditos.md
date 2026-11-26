# Estilos prontos da barra e do Iniciar: origem e créditos

Os estilos prontos da tela Estilização (barra de tarefas e menu Iniciar) usam só valores lidos dos
repositórios abaixo (cores, opacidade, cantos e margens). Nenhum código deles foi copiado.
O método do Sentinela (XAML Diagnostics, o mesmo do Windhawk) troca cor, opacidade, cantos,
margens (barra flutuante) e borda do fundo da barra; não reproduz desfoque, gradiente, imagem
nem o layout de docks. Presets "aproximados" dizem isso na tela.

| Repositório | O que entrou |
| --- | --- |
| [ramensoftware/windows-11-taskbar-styling-guide](https://github.com/ramensoftware/windows-11-taskbar-styling-guide) | Tintas de TranslucentTaskbar (Undisputed00x), TintedGlass (TheRealCisWhiteMale), Plasma (Sand216), Oversimplified&Accentuated (OsamaJT) e o fundo transparente de SimplyTransparent (Osprey00), RosePine, Squircle, xdark, BottomDensy e Matter |
| [abdullah-al-faahim/Windows-11-Customization](https://github.com/abdullah-al-faahim/Windows-11-Customization) | Paleta Catppuccin Mocha (`#1e1e2e`, cantos 6) de `configs/windhawk` |
| [Icetist/Astraeus](https://github.com/Icetist/Astraeus) (GPL-3.0) | Cores das aparências Starfall e clara, calculadas do HSL em `src/theme.cpp` |
| [Tanmay-Tiwaricyber/HyperLand](https://github.com/Tanmay-Tiwaricyber/HyperLand) (MIT) | Vidro escuro translúcido inspirado no visual Liquid Glass (valores aproximados) |
| [dantejvar28/widdgyy](https://github.com/dantejvar28/widdgyy) | Paletas dos showcases aurora, bonsai, neon, neon-divide e night-train (`styles.css`) |

## Não aplicáveis pelo método atual

Lucent, WinXP, Windows7, WinVista (gradiente ou imagem); CleanSlate, Aeris, TaskbarToStatusbar
(acrílico e cor de destaque do sistema); Pills, Blob, Bubbles, DockLike, FluentGlass, FrostyGlass,
LiquidGlass2, OS26 Liquid Glass, One UI 8.5, Surface, TaskbarXII, UltraWideFriendly, WindowGlass,
Minecraft Hotbar, LayerMicaUI (recriam o layout da barra); 21996Taskbar, SunValley, Fluid,
LiquidGlass, Command Center, Borderless, Luminosity (botões, bandeja, miniaturas). As ilhas do
Astraeus e do HyperLand são aplicativos próprios, não estilos da barra.

## Visão "Todos os aplicativos" do Iniciar

Estilização > Menu Iniciar > "Visão de Todos os aplicativos": grava
`HKCU\Software\Microsoft\Windows\CurrentVersion\Start\AllAppsViewMode` (0 categoria, 1 grade,
2 lista). Fonte: guia do Geek Rewind (uma fonte só; não testado no PC). Pede reinício do Explorer.

## Tiling, Snap e monitores em retrato

- Janela que você arrasta ou encaixa (Snap, Win+setas) sai do tiling e fica onde está; o atalho
  `toggle-float` a traz de volta. O tiling espera o botão do mouse soltar antes de reorganizar.
- Ação `snap-layouts` (Win+Z): abre os layouts de encaixe do Windows.
- Monitores em retrato usam `portrait_layout` (padrão `rows`, janelas empilhadas). Por monitor:
  `[[tiling.monitor]]` com `index` (1 = mais à esquerda), `layout` e `max_windows` (2 ou 3 zonas).
