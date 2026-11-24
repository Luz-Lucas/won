# Testes manuais dos mods 4 (ícones pequenos) e 5 (rótulos)

Build testado: Windows 11 25H2, Taskbar.dll 10.0.26100.9818. Ponto de restauração anterior: nº 215.
Pasta: `%LOCALAPPDATA%\Sentinela\mods\bin` (sentinela-mods.exe e as DLLs).

1. `sentinela-mods shell-labels sentinela_mod_labels.dll --experimental` — rótulos: botões sem agrupar.
2. `sentinela-mods eject-all` — a barra volta ao que era (a DLL restaura os bytes originais).
3. `sentinela-mods shell-icons sentinela_mod_icons.dll --experimental` — ícones pequenos.
4. Abra e feche janelas para a barra redesenhar; se nada mudar, o Windows 11 não consulta aquela função
   neste build e o hook precisa de outro ponto (anote no PR).
5. Recuperação: `recuperacao\desfazer-mods.cmd`, ou `sentinela-mods eject-all`.

Ainda não feitos: mods 6 e 7 e a fase 13 (dependem da API de XAML) e a fase 14 (exige confirmação explícita).
