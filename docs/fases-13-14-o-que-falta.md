# Fases 13 e 14 e mods 4 a 7: o que já existe e o que falta

## Já pronto e testado (módulo `sentinela_mods::shell`)
As travas do plano, em lógica pura coberta por testes:
- lista fechada de processos (`explorer.exe`, `StartMenuExperienceHost.exe`, `ShellExperienceHost.exe`);
- builds do Windows testados por mod; build desconhecido só liga como "experimental";
- modo seguro: duas quedas do Explorer em até 60 s depois do carregamento desligam tudo;
- parada em jogos e anticheat (descarrega ao abrir, recarrega quando o último fecha);
- resolução de símbolos por PDB oficial (fase 12) e script de recuperação (fase 11).
`may_enable` só libera um mod se todas as travas liberarem; qualquer dúvida bloqueia.

## O que falta e por que não dá para fazer daqui
1. **Hooks reais dos mods 4 a 7** (altura da barra, rótulos, estilo da barra, estilo do Iniciar): é preciso descobrir, em cada build do Windows 11, quais funções de `explorer.exe` e `Taskbar.dll` mexem em altura e rótulos. Isso se faz com depurador e o PDB num Windows 11 real; o nome dessas funções não está documentado e não pode ser chutado, porque um nome errado liga um hook no lugar errado.
2. **Mods XAML (fase 13)**: exigem a API de diagnóstico de XAML dentro de `explorer.exe`, também só verificável em sessão gráfica real.
3. **Temas .msstyles (fase 14)**: o Windows só aceita temas com assinatura válida. Desligar essa verificação exige um hook em `uxtheme.dll`/`themeui.dll` no logon: é a peça com maior risco de tela preta e fica fora enquanto não houver uma VM de teste com snapshot.
4. **Runner do CI não basta**: o `windows-latest` não tem sessão gráfica interativa com Explorer, então ele não prova nada sobre hooks do shell.

## O que eu preciso para continuar
Uma VM Windows 11 com sessão gráfica e snapshot (Hyper-V ou VirtualBox), com acesso para eu rodar o roteiro e restaurar o snapshot se o login travar. Com isso, cada mod entra com roteiro, trava de build e script de recuperação, um de cada vez.
