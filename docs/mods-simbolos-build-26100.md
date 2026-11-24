# Símbolos dos mods 4 a 7: build 10.0.26100.9818 (Windows 11 25H2)

Levantado no PC do Lucas com o PDB público oficial da Microsoft (baixado pelo
`sentinela-mods symbols`) e listado com `tools/enum-pdb-symbols.ps1` (dbghelp).
`explorer.exe` tem 16164 símbolos públicos e `Taskbar.dll` tem 25149.

**Isto é só um índice de nomes.** Existir o nome não prova o que a função faz, nem o
protótipo. Nenhum hook foi ligado. Antes de ligar, cada função precisa ser desmontada
(ou observada num depurador) para confirmar argumentos e retorno.

## Candidatos em `Taskbar.dll` (RVA)
| Mod | Função | RVA |
|---|---|---|
| 4 altura da barra | `TrayUI::UpdateTaskbarSize` | 0x1021E0 |
| 4 altura da barra | `TrayUI::UpdateSecondaryTaskbarSize` | 0x102120 |
| 4 altura da barra | `TaskbarHost::FrameHeight` | 0xB10A4 |
| 4 altura da barra | `TaskbarHost::TryUpdateVerticalAlignmentAndGridHeight` | 0x13A534 |
| 4 altura da barra | `CTaskBand::_VerifyButtonHeight` | 0x6A97C |
| 4 tamanho dos ícones | `TrayUI::GetIconSizePreference` | 0xF96B0 |
| 4 tamanho dos ícones | `TrayUI::ShouldUseSmallIcons` | 0x63020 |
| 4 tamanho dos ícones | `CTaskBand::GetIconSize` | 0x123010 |
| 4 tamanho dos ícones | `TaskbarHost::IconSizeInViewPixels` | 0x75CAC |
| 5 rótulos | `GetTaskbarGlomLevelFlags` | 0x63030 |
| 5 rótulos | `CTaskListWnd::GetButtonHeight` | 0x63950 |
| 5 rótulos | `CTaskListWndMulti::GetButtonHeight` | 0xA45E0 |

Mods 6 e 7 (estilo da barra e do Iniciar) não aparecem como função nativa: no Windows 11
o visual é XAML (fase 13), então dependem da API de diagnóstico de XAML dentro do Explorer.

## Fase 14
`uxtheme.dll` e `themeui.dll` existem em System32 mas ainda não tiveram símbolos levantados.
Fica por último e só com confirmação explícita do Lucas.

## Pendências para ligar qualquer hook
- Toolchain Rust e um desmontador/depurador neste PC (nenhum instalado hoje).
- Ponto de restauração "Sentinela mods 4-7 antes de injecao" já criado (seq. 215).
