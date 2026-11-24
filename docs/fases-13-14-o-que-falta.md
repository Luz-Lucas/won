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

## Sessão de depuração ao vivo no Explorer do Lucas (sem risco: anexação não invasiva)
Anexei o WinDbg (`cdb -pv`, modo não invasivo: não suspende o processo, só lê) no `explorer.exe`
rodando no PC do Lucas e saí sem alterar nada (`.detach`). O Explorer continuou com o mesmo PID antes
e depois. Isso confirmou o módulo real que falta:

- `Taskbar.View.dll` (em `C:\WINDOWS\SystemApps\MicrosoftWindows.Client.Core_cw5n1h2txyewy\`), carregado
  dentro do `explorer.exe`. É aqui, não em `Taskbar.dll`, que o visual do Windows 11 é desenhado. Baixei
  o PDB oficial dele e listei 169731 símbolos.

### O que achei para o mod 6 (estilo da barra)
A classe `winrt::Taskbar::implementation::TaskbarBackground` tem os métodos reais:
- `put_TaskbarTheme` (RVA 0x22EE20) e `TaskbarTheme` (0x22EE4C) — tema claro/escuro/acrílico da barra;
- `put_IsBorderVisible` / `get_IsBorderVisible` (0x23D5D0 / 0x265B70);
- `AcrylicThemeBrush` / `AccentAcrylicThemeBrush` (0x21B660 / 0x21B5D4) — a cor/transparência real;
- `UpdateVisualStates` (0x22EE9C) e `OnColorValuesChanged` (0x1B9AF0) — recalculam tudo isso.

### Por que não dá para ligar isso com um patch de bytes (como os mods 4 e 5)
Essas funções são métodos de **objeto WinRT/COM** de uma instância que já existe dentro do Explorer
vivo, não funções `extern "C"` isoladas. Chamar `put_TaskbarTheme` de fora exige:
1. achar o ponteiro real da instância de `TaskbarBackground` que está na tela agora (não documentado,
   varia por execução, precisa de uma busca pela árvore de objetos XAML em tempo real);
2. respeitar o layout da vtable COM daquele objeto, que não vem no PDB (o PDB dá endereço de função,
   não o índice na vtable nem o `this` certo).
Um endereço de função sem o `this` correto não é "ligar o mod", é escrever lixo numa estrutura COM
viva dentro do processo que desenha a tela toda — pode corromper memória do Explorer sem aviso, o
que é um risco bem maior que os patches dos mods 4 e 5 (ali eu só troco o início de uma função que
não depende de instância nenhuma).

### Conclusão sobre a fase 13 e os mods 6 e 7
Fica confirmado, agora com nomes e endereços reais, o que o plano original já previa: esta parte
precisa de uma ferramenta de diagnóstico de árvore XAML (o "Live Visual Tree" do Visual Studio, que
não está instalado e não é scriptável por uma sessão automatizada) para achar o `this` certo em tempo
real, não apenas do PDB. Não tentei chutar o `this`, porque isso é exatamente o tipo de hook que o
`may_enable` do shell foi desenhado para recusar (`SymbolsMissing` cobre o endereço da função; não
cobre, e não poderia cobrir, "ponteiro de instância certo").

## Fase 14 aplicada no disco (PC de testes do Lucas, com o risco aceito por ele)
Patch real aplicado em `C:\Windows\System32\uxtheme.dll` (não cópia, não injeção): `CThemeSignature::Verify`
(RVA 0x624BC, ver acima) teve o prólogo trocado por `xor eax,eax; ret` (sempre "assinatura válida").
Passos tomados, nessa ordem:
1. Backup dos originais com hash em `%LOCALAPPDATA%\Sentinela\mods\backup-sistema\*.original`.
2. Ponto de restauração nº 216 ("Sentinela fase 14: antes do patch de uxtheme.dll").
3. Verifiquei que esta máquina não tem Memory Integrity/HVCI ativo (`Win32_DeviceGuard`), o que reduz
   bastante o risco de o Windows recusar carregar o arquivo modificado por integridade de código.
4. `takeown` + `icacls` para tomar posse do arquivo (protegido por TrustedInstaller).
5. Escrita no arquivo não foi direta (o DLL estava em uso): gravei a versão nova num arquivo temporário
   e troquei por `Move-Item` (renomear por cima funciona mesmo com o DLL mapeado; processos que já tinham
   o arquivo aberto continuam com a versão antiga até reiniciarem).
6. Confirmei os bytes gravados e reiniciei o Explorer: subiu normal com o arquivo novo já carregado.

**Ainda não testado:** sobrevivência a um reboot completo (login). Isso só um reboot real confirma.
`%LOCALAPPDATA%\Sentinela\mods\backup-sistema\uxtheme.dll.original` + ponto nº 216 desfazem se algo falhar.

## Atualização: mod 6 (estilo da barra) feito pelo método do Windhawk
A abordagem de vtable/`this` foi descartada. `crates/mod-xaml` (`sentinela_mod_xaml.dll`) usa a API de
diagnóstico do XAML (`InitializeXamlDiagnosticsEx` + `IVisualTreeService`), a mesma do Live Visual Tree:
recebe cada elemento da árvore, acha `BackgroundFill`/`BackgroundStroke` dentro de `TaskbarFrame` e troca
`Fill`, `Margin`, `RadiusX/Y` e a opacidade da borda por APIs WinRT, na thread de UI. Nada é escrito em
memória do Explorer. Testado no build 26100/26200: aplicou nas duas barras (monitor principal e
secundário), o Explorer manteve o PID, e o eject restaurou a aparência original.

Uso: `sentinela-mods shell-xaml <dll> --experimental`; configuração em `%LOCALAPPDATA%\Sentinela\mods\xaml-barra.cfg`
(`fill`, `radius`, `margin`, `hide_border`, `enabled`), relida a cada 0,5 s; desfazer: `sentinela-mods eject-all`
(cria `xaml-barra.eject`, a DLL restaura os valores originais). A DLL se fixa no processo porque o XAML guarda
ponteiros para ela; reiniciar o Explorer a remove por completo. Diagnóstico em `xaml-barra.log`.
Bindings WinRT gerados com `windows-bindgen` a partir de `Windows.winmd` do SDK 26100 (o crate `windows` 0.62 não traz `UI.Xaml`).
