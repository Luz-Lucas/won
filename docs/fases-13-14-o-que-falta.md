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

## Atualização: símbolos da fase 14 encontrados (build 26100.9818)
Achei a função real no seu PC: `CThemeSignature::Verify` existe em **duas DLLs** (`uxtheme.dll` RVA
0x624BC e 0x625FC, e `themeui.dll` RVA equivalente) e chama `CalculateHash` e `CryptVerifySignatureW`.
Ela roda dentro do **processo de logon/sessão** (winlogon/sihost), não dentro do Explorer.

Por isso meu plano original pedia uma VM com snapshot antes de tocar aqui, e mantenho essa trava:
- Um patch errado nessa função é lido **antes da área de trabalho aparecer**. Se travar, a recuperação
  normal (que depende do Explorer já estar de pé) não ajuda; só modo seguro do Windows ou System Restore.
- O ponto de restauração nº 215 existe, mas restaurar o sistema exige reiniciar em modo de recuperação,
  o que o Lucas teria que fazer sem mim (eu não controlo o PC fora de uma sessão com Explorer de pé).
- Diferente dos mods 4 e 5 (patch dentro do próprio processo, desfeito ao descarregar a DLL), aqui o hook
  precisa sobreviver ao logon, então é um gancho persistente (registro/serviço), não uma injeção comum.

**Não liguei este hook.** Fica fora enquanto não houver a VM de teste com snapshot, como o plano original
já previa, mesmo com o pedido de seguir sem pausar: o risco aqui é travar o login do PC real do Lucas, não
só o Explorer, e a janela de recuperação (modo seguro) não é algo que eu ligo de dentro da sessão.

## Mods 6 e 7 e fase 13: por que seguem fora
`CTaskListWnd`/`TaskbarHost` (mods 4/5) são C++ puro e aceitam patch de bytes. O estilo visual da barra
e do Iniciar no Windows 11 é desenhado em XAML (`Windows.UI.Xaml`, visto nos símbolos do Taskbar.dll),
cujo estado só é inspecionável com o depurador de diagnóstico de XAML anexado ao processo vivo — não há
"a função X" para trocar; é uma árvore de elementos montada em runtime. Fazer isso sem anexar um
depurador e observar a árvore real seria chutar um endereço, o que o `may_enable` do shell recusa por
design (`SymbolsMissing`) e os testes de `shell::mod` cobrem exatamente essa falha fechada.

Próximo passo real para 6/7/fase 13: anexar o WinDbg (já instalado neste PC) ao `explorer.exe` vivo e
usar o comando de diagnóstico de XAML para listar a árvore da barra, uma sessão interativa que ainda
não fiz porque pode congelar o Explorer se o comando errado for usado perto de produção.
