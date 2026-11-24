# Fase 11: motor de mods (ainda não executado no seu PC)

O CI (runner `windows-latest`, VM descartável) injeta uma DLL de teste inofensiva (`sentinela-mod-test`) num `ping.exe`, confere o marcador "attached", descarrega, confere "detached" e roda o script de recuperação. Isso está em `crates/mods/tests/inject_ci.rs`.

## O que o CI não prova
Login real, tela, anticheats e o comportamento de qualquer DLL de terceiros. Teste só você, no seu PC.

## Regras fixas
- Alvos precisam estar em `targets` no `mods.toml`. Sem `targets`, nada é carregado.
- Nunca: explorer.exe, winlogon, csrss, lsass, svchost, dwm e outros do sistema, nem o próprio Sentinela.
- Nunca com anticheat aberto (EasyAntiCheat, BattlEye, Vanguard, FACEIT etc.): a injeção é recusada.
- Só processos de 64 bits.

## Uso (PowerShell)
1. Crie `%LOCALAPPDATA%\Sentinela\mods\mods.toml`:
   ```toml
   [[mod]]
   name = "meu-mod"
   dll = "C:\\caminho\\meu.dll"
   targets = ["notepad.exe"]
   ```
2. `sentinela-mods list`, depois `sentinela-mods inject meu-mod` com o Bloco de Notas aberto.
3. `sentinela-mods eject meu-mod` descarrega.

## Recuperação
Ao primeiro `inject`, o script `%LOCALAPPDATA%\Sentinela\mods\recuperacao\desfazer-mods.cmd` é gravado. Ele descarrega tudo, apaga a entrada de inicialização e renomeia `mods.toml` para `mods.toml.desativado`. Roda sem o Sentinela abrir e no modo seguro. `sentinela-mods recovery` regrava e mostra o caminho.

## Tela de Mods (menu "Mods")
Mostra os mods do `mods.toml`, os programas permitidos, onde cada um está carregado, o aviso de anticheat aberto, e os botões Carregar (com confirmação), Descarregar, Descarregar todos e "Gravar e mostrar o script de recuperação". O cadastro continua sendo o arquivo `mods.toml`; a tela não edita DLLs nem programas permitidos.
