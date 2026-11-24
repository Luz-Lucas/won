# Fase 12: resolução de símbolos (ainda não executado no seu PC)

O CI baixa o PDB oficial do `ntdll.dll` do runner, resolve `NtClose` e confere o endereço com a tabela de exportação do próprio arquivo, depois repete pelo cache e confirma que um nome inexistente derruba tudo (falha fechada).

## No seu PC
1. `sentinela-mods symbols C:\Windows\System32\ntdll.dll NtClose` imprime `NtClose = 0x...`. A primeira execução baixa o PDB de `msdl.microsoft.com` (precisa de internet); as seguintes usam o cache em `%LOCALAPPDATA%\Sentinela\mods\symbols`.
2. Um nome que não existe faz o comando falhar sem imprimir nada.

## Mods 4 e 5 (altura da barra, rótulos dos botões)
**Não implementados de propósito.** Dependem de instalar hooks dentro do `explorer.exe`, que a política do motor recusa (lista de processos protegidos). Liberar isso exige uma VM com Windows 11 real e sessão gráfica para validar, e um erro derruba o shell no logon. Ficam para quando houver esse ambiente de teste.
