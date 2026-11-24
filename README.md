# Sentinela

Antivírus e limpador de arquivos para Windows, escrito em Rust, com janela (Tauri) e linha de comando. Varre arquivos com assinaturas de hash (SHA-256), regras YARA e heurísticas de executáveis PE, além de limpar lixo do sistema e encontrar arquivos grandes e duplicados. Distribuído por um instalador do Windows que traz a janela e o `sentinela.exe` (linha de comando).

> **Aviso importante:** o Sentinela é um projeto complementar e **não substitui** um antivírus completo. **Mantenha o Windows Defender (Segurança do Windows) sempre ativo.**

> **Falsos positivos:** regras YARA da comunidade e heurísticas de PE podem marcar arquivos legítimos como suspeitos. Um achado `[SUSPEITO]` é um alerta para você investigar, não uma sentença: confira o arquivo (por exemplo no VirusTotal) antes de apagá-lo. O Sentinela **nunca apaga nem move** arquivos encontrados pela varredura.

## Download

Baixe o instalador `Sentinela_<versão>_x64-setup.exe` mais recente na página de [releases](https://github.com/Luz-Lucas/sentinela/releases). Ele instala para todos os usuários (em `C:\Program Files\Sentinela`, pede permissão de administrador) e coloca lado a lado a janela (`sentinela-gui.exe`) e o `sentinela.exe`. Não há versão portátil. A release também anexa o `sentinela.exe` solto, para quem só quer a linha de comando.

### Interface gráfica

A janela do Sentinela tem estas páginas, na barra lateral:

- **Início**: situação da proteção, atalhos para verificar e liberar espaço, e a atividade recente. Com as atualizações do Windows bloqueadas ou pausadas, mostra há quantos dias o Windows está sem correções e o botão **"Atualizar agora"** (libera, abre o Windows Update e depois oferece voltar ao modo).
- **Jogos**: otimizações do Windows com prévia, perfis Recomendado, **FPS competitivo** (Recomendado + os situacionais) e Personalizado, e ponto de restauração. A aba **Avançado** traz os ajustes situacionais e controversos, cada um com a explicação inteira; os controversos nunca entram em perfil. A seção **GPU por jogo** liga a placa de alto desempenho em cada jogo detectado (Steam, Epic, Riot ou "Adicionar jogo…"). No fim, **Programas que iniciam com o Windows**: liga e desliga cada programa como o Gerenciador de Tarefas, sem apagar nada; entradas de todos os usuários pedem administrador, e programas cujo arquivo sumiu aparecem como "Arquivo não encontrado".
- **Limpeza**: simula a limpeza e mostra, por categoria, o que seria apagado e o espaço liberado. Nada é apagado antes de você confirmar; categorias desmarcadas são mantidas.
- **Aplicativos**: instala e atualiza aplicativos do catálogo pelo winget.
- **Personalizar**: barra de tarefas, menu Iniciar, Explorador, tema e sistema, com interruptores e listas de opções. Cada mudança mostra a prévia, pede confirmação e pode ser desfeita pelo Histórico. Mudanças que só aparecem com o Explorer reiniciado oferecem o botão "Reiniciar Explorer"; o Sentinela nunca o reinicia sozinho. Duas seções novas: **Papel de parede** (só aparece com o Wallpaper Engine da Steam instalado) lista a sua Oficina com miniaturas, troca o wallpaper pelo `-control` oficial do programa e **pausa ou para o wallpaper enquanto um jogo detectado estiver aberto** (volta sozinho ao fechar o jogo ou o Sentinela; a opção "Não mexer" desliga isso). O Sentinela só lê a Oficina e a configuração do Wallpaper Engine; nunca grava nelas. A seção **Temas** traz os prontos *Minimalista* e *Translúcido*, mostra a prévia de tudo que mudaria antes de aplicar (ajustes nativos, cor de destaque do Windows, que pode vir da miniatura do wallpaper, e o wallpaper da Oficina) e **desfaz o tema inteiro de uma vez**, inclusive o papel de parede anterior. **Exportar** salva o seu visual atual num arquivo `.sentinela-theme` e **Importar** lê um arquivo de outra pessoa: o arquivo só pode listar ids de ajustes do catálogo, o número de um wallpaper da Oficina e uma cor. Caminhos de entrada com `..`, arquivos grandes demais, ajustes desconhecidos ou que pedem administrador e seções que um dia poderão rodar comandos são recusados ou vêm desligados; se o wallpaper do tema não está na sua Oficina, o aviso traz o link para assiná-lo. A seção **Terminal** (aparece quando há PowerShell ou Windows Terminal) instala o **Oh My Posh** pelo winget, só para o seu usuário, e a fonte Nerd Font pelo próprio `oh-my-posh font install` (Meslo por padrão). Você escolhe um tema do prompt, vê a amostra e o tempo que ele leva para desenhar, e **vê o diff exato de cada arquivo antes de aplicar**. O Sentinela só escreve num bloco marcado (`# >>> sentinela >>>`) do perfil do PowerShell e nas chaves que gerencia no `settings.json` do Windows Terminal (fonte e um esquema de cores gerado a partir do wallpaper, com contraste conferido); se não tiver certeza de que a edição é segura, recusa. Segmentos de tema que rodam comandos (`command`, `script`, `executable`, `http`) vêm desligados. Cada arquivo tem cópia antes da primeira mudança e as mudanças se desfazem uma a uma; se você editou o arquivo depois, o Sentinela avisa antes de desfazer. Os temas do Sentinela também podem levar fonte, cores e prompt do terminal. Git Bash e cmd não são mexidos.
- **Rede**: mede latência, *jitter* e perda até o roteador, a Cloudflare e os servidores que você acrescentar; rastreia a rota separando sua casa do provedor e mostra onde o atraso começa; testa o *bufferbloat*; compara o DNS atual com DNS públicos e, se outro for mais rápido, aplica com prévia (desfaz pelo Histórico); desliga a economia de energia do adaptador de rede; mostra o sinal do Wi‑Fi. Em **Rotas alternativas**, compara a conexão atual com o Cloudflare WARP e com um túnel WireGuard seu (e mostra ExitLag/NoPing, se instalados), só recomenda quando a diferença passa da variação medida, liga a rota escolhida ao abrir cada jogo e pode segurar a varredura do Wi‑Fi durante jogos. Na troca de DNS, oferece DNS sobre HTTPS quando o Windows conhece o servidor. A rota até o servidor do jogo é do provedor: o Sentinela mede e ajusta só o que é do computador.
- **Segurança**: antivírus sob demanda (sem proteção em tempo real; mantenha o Defender ligado). A verificação **Rápida** lê os programas abertos, tudo que inicia com o Windows (chaves `Run`/`RunOnce`, pastas Inicializar, tarefas agendadas e serviços) e as pastas onde malware costuma cair (Downloads, Área de Trabalho, Documentos, Temp, AppData, ProgramData, Público e a raiz dos discos), só nos tipos de arquivo de risco; a **Completa** lê todos os discos fixos; a **Personalizada**, as pastas e discos que você escolher. Cada achado diz por quê em português, com o SHA-256, e tem Detalhes, Abrir pasta, Consultar no VirusTotal (só o hash vai ao navegador) e Ignorar sempre. Só o malware confirmado por hash vem marcado para quarentena; nada é movido sem você pedir. Antes de mover, arquivos da pasta do Windows pedem uma segunda confirmação e arquivos de programas abertos oferecem encerrar o programa. Depois da primeira verificação, oferece agendar uma rápida por semana. Além dos níveis Malicioso e Suspeito há o nível **Risco**: nada casou, mas o arquivo está num contexto ruim (executável sem assinatura válida que inicia sozinho a partir de Temp ou AppData, extensão dupla como `foto.jpg.exe` ou programa disfarçado de documento, script na pasta Inicializar, atalho que chama o PowerShell com comando codificado, Office com macro que roda ao abrir). Os de Risco nunca vêm marcados. A **assinatura digital (Authenticode)** é validada de verdade: assinatura válida tira pontos da heurística de executáveis (nunca anula hash de malware conhecido nem regra YARA) e arquivos assinados pela Microsoft pedem a mesma segunda confirmação da pasta do Windows antes da quarentena. Arquivos **.zip** são verificados por dentro (até 10.000 entradas, 64 MB por entrada, 512 MB no total, dois níveis de zip dentro de zip e compressão até 200:1; o que passa do limite ou tem senha aparece em "Não verificados"); a quarentena move o compactado inteiro e a janela avisa antes. A janela guarda um **cache** (`%LOCALAPPDATA%\Sentinela\scan-cache.bin`) dos arquivos limpos: o que não mudou não é relido, mas o hash é sempre conferido contra o banco, e regras novas descartam o cache. A verificação da janela roda em **segundo plano** (metade dos processadores, prioridade baixa de CPU e disco), pode ser **pausada**, e pausa sozinha enquanto um jogo detectado estiver aberto. Em Ajustes, "Verificar com o Sentinela" no **clique direito** do Explorador (arquivos, pastas e discos; no Windows 11, em "Mostrar mais opções") começa uma verificação personalizada do item. Sem administrador, a janela diz o que ficou de fora por acesso negado e oferece "Reiniciar como administrador".
- **Quarentena**: os arquivos ficam guardados codificados (não rodam por engano); restaurar confere o conteúdo e nunca sobrescreve; apagar definitivamente pede confirmação.
- **Disco**: lista os maiores arquivos e os duplicados de uma pasta. Só lista; nunca apaga.
- **Histórico**: as otimizações e personalizações aplicadas, com desfazer.
- **Ajustes**: atualiza o banco de assinaturas (e o atualiza sozinho ao abrir o app quando tem mais de 24 h), guarda a chave gratuita da abuse.ch e define a inicialização com o Windows e a bandeja. Em **Verificação agendada**, liga a rápida semanal (domingo ao meio-dia pelo Agendador de Tarefas, sem administrador; se o computador estava desligado, roda assim que ligar, e o resultado vem numa notificação) e a quarentena automática do malware confirmado por hash, desligada por padrão. **Exceções da verificação** lista o que está em "Ignorar sempre". A seção **Atualizações do Windows** escolhe entre Padrão, Só segurança (recomendado), Pausado até… e Bloqueado, com prévia e aviso dos efeitos; mostra "Parcial" quando o Windows desfaz algo sozinho, com "Aplicar de novo". Mudar o modo exige administrador.

O rodapé da barra lateral mostra se o programa está como administrador e oferece "Reiniciar como administrador". As regras de segurança são as mesmas do `sentinela.exe` descrito abaixo.

### Linha de comando

Depois de instalar, o `sentinela.exe` fica na pasta de instalação (`C:\Program Files\Sentinela`); adicione-a ao `PATH` ou use o caminho completo.

## Comandos

```
sentinela scan <caminho>... [--json] [--max-size <MB>] [--exclude <glob>]...
sentinela scan --quick [--json]
sentinela update
sentinela clean [--category <nome>]... [--apply] [--yes] [--json]
sentinela find-large <caminho> [--top <N>] [--min-size <MB>] [--json]
sentinela find-dupes <caminho> [--min-size <KB>] [--json]
sentinela full [--json]
sentinela quarantine [--json]
sentinela quarantine --delete <id>
sentinela restore <id>
sentinela tweak list | apply <id>... | undo (<id>... | --all)
sentinela apps list | install <id>... | upgrade (<id>... | --all)
sentinela startup list | enable <id> | disable <id>
sentinela updates status | mode <modo> [--until AAAA-MM-DD]
sentinela games list | gpu (on | off) <id>
sentinela net test [<destino>...] | trace <destino> | bufferbloat | dns | history
sentinela net routes | compare <destino> [--route <id>]... | route <id> (on | off)
sentinela info
```

### Códigos de saída

| Código | Significado |
| --- | --- |
| `0` | Sucesso / nada encontrado |
| `1` | Achados de malware (`scan` encontrou algo suspeito ou malicioso, o nível `[RISCO]` não conta; no `full`, só quando malware confirmado por hash **não** pôde ser isolado) |
| `2` | Erro global (caminho inexistente, categoria inválida, falha de rede no `update`, uso incorreto) |

Erros em arquivos individuais (acesso negado, arquivo em uso) **não** interrompem a execução: aparecem em "Não verificados" no texto ou em `errors` no JSON.

### `scan`: verificar arquivos e pastas

```
> sentinela scan C:\Users\eu\Downloads
Verificados: 1342 | Maliciosos: 1 | Suspeitos: 1 | Risco: 1 | Erros: 2
[MALICIOSO] C:\Users\eu\Downloads\crack.exe — sha256:3f9a…
[SUSPEITO] C:\Users\eu\Downloads\setup.exe — pe-heuristics
[RISCO] C:\Users\eu\Downloads\foto.jpg.exe — double-extension

Não verificados:
  C:\Users\eu\Downloads\bloqueado.tmp — acesso negado
```

- `--json`: saída estável em JSON, um objeto com `version` (1), `summary` (com `malicious`, `suspicious` e `risk`), `items` e `errors`.
- `[RISCO]` é o nível abaixo de `[SUSPEITO]` (contexto ruim, sem nada casar) e **não** muda o código de saída. O CLI valida a assinatura Authenticode e verifica `.zip` por dentro, mas não usa o cache de verificados.
- `--max-size <MB>` (padrão `256`): arquivos maiores que isso só têm o hash calculado (em streaming), sem análise de conteúdo.
- `--exclude <glob>` (repetível): ignora caminhos que casam com o glob, por exemplo `--exclude "**/node_modules/**" --exclude "**/*.iso"`.
- Links simbólicos e junctions nunca são seguidos.
- Barra de progresso: em um terminal, o `scan` (e a fase de verificação do `full`) mostra no stderr uma linha que se atualiza no lugar, com porcentagem (por bytes), arquivos, velocidade, ETA e o arquivo atual. Ela some ao terminar e fica oculta com `--json` ou quando o stderr é redirecionado/canalizado.
- `Ctrl+C` interrompe a varredura e imprime o relatório parcial.

```
sentinela scan C:\Users\eu\Downloads D:\Projetos --max-size 100 --exclude "**/*.iso" --json
```

`sentinela scan --quick` faz a verificação rápida da janela: programas abertos, o que inicia com o Windows e as pastas de risco, só nos tipos de arquivo de risco (executáveis pelo cabeçalho, scripts, instaladores, atalhos, Office com macro e compactados). Sem administrador, as tarefas agendadas aparecem em "Não verificados". O banco de assinaturas e a quarentena do próprio Sentinela nunca são verificados.

### `full`: verificar o computador inteiro, isolar malware e limpar

```
> sentinela full
Verificando: C:\, D:\. Isso pode demorar bastante (todos os discos fixos).
Verificados: 912340 | Em quarentena: 1 | Suspeitos (só relatados): 1 | Erros: 37

Quarentena (malware confirmado por hash):
  C:\Users\eu\Downloads\crack.exe — sha256:3f9a… (id 9c1e…; restaurar: sentinela restore 9c1e…)

Suspeitos (apenas relatados, nenhum arquivo foi alterado):
  [SUSPEITO] D:\Jogos\setup.exe — pe-heuristics

Limpeza:
…
```

Um único comando, sem argumentos e **sem perguntas**:

1. Verifica **todos os discos fixos** (`C:\`, `D:\`, …), não apenas algumas pastas. Pode levar muito tempo; rode como administrador para cobertura total. Discos removíveis, de rede e ópticos são ignorados.
2. Move para a quarentena **somente** arquivos cujo hash é de malware conhecido (detector `hash`). Achados de YARA ou de heurísticas de PE (`[SUSPEITO]`, ou `[MALICIOSO]` vindo de YARA) podem ser falsos positivos: são **apenas relatados**, o arquivo não é tocado.
3. Executa a limpeza de **todas** as categorias do `clean` com `--apply`, **automaticamente e sem confirmação** (as mesmas regras de segurança do `clean`: só arquivos antigos, nunca links). Se você interromper com `Ctrl+C`, a limpeza não é executada.
4. Durante a verificação mostra a mesma barra de progresso com ETA do `scan` (oculta com `--json` ou quando o stderr não é um terminal).
5. Imprime o que foi isolado (caminho, regra, id), o que foi apenas relatado e o detalhamento da limpeza. `--json` produz o mesmo relatório em JSON.

Código de saída: `0` normalmente (itens em quarentena ou suspeitos não alteram o código); `1` se malware confirmado por hash não pôde ser movido e continua no disco; `2` em erro global (por exemplo, nenhum disco encontrado). Falhas em arquivos individuais aparecem em "Não verificados".

### `quarantine` e `restore`: quarentena reversível

A quarentena fica em `%LOCALAPPDATA%\Sentinela\quarantine` (altere com a variável `SENTINELA_QUARANTINE_DIR`): os arquivos isolados em `files\<id>` (gravados codificados, para não rodarem por engano; os de versões anteriores continuam restauráveis) e o índice em `index.json`. Os arquivos são **movidos**, e só `--delete` (ou "Apagar" na página Quarentena) os apaga de vez: a movimentação funciona entre discos diferentes (copia, confere o hash e só então remove o original) e, se algo falhar, o original continua no lugar. Links simbólicos e junctions nunca são seguidos.

```
sentinela quarantine [--json]       # lista id, data, tamanho, caminho original e regra
sentinela restore <id>              # devolve o arquivo ao caminho original
sentinela quarantine --delete <id>  # apaga de vez (não tem volta)
```

O `restore` recusa-se a sobrescrever: se já existir algo no caminho original, ele falha (código `2`) e mantém a cópia em quarentena. Confere também o hash antes de devolver o arquivo.

### `update`: baixar assinaturas

```
> sentinela update
Banco atualizado em C:\Users\eu\AppData\Local\Sentinela\db
  Hashes: 48213
  Regras YARA: 112 arquivo(s) aceito(s), 3 rejeitado(s), 640 regra(s) no total
```

O banco fica em `%LOCALAPPDATA%\Sentinela\db` (altere com a variável `SENTINELA_DB_DIR`; o `update` recusa substituir uma pasta existente, não vazia, que não pareça um banco do Sentinela — sem `manifest.json`, `hashes.txt` ou `rules/` — então aponte para uma pasta vazia ou inexistente). Se o banco ainda não foi inicializado, o `scan` avisa no stderr. Sem rodar `update`, o `scan` ainda funciona com a assinatura embutida do arquivo de teste EICAR.

Os hashes vêm do MalwareBazaar (abuse.ch), que exige uma chave de autenticação **gratuita**: crie uma conta em <https://auth.abuse.ch/> e defina a variável de ambiente `ABUSE_CH_AUTH_KEY`:

```
> set ABUSE_CH_AUTH_KEY=sua-chave
> sentinela update
```

No PowerShell: `$env:ABUSE_CH_AUTH_KEY = "sua-chave"`. Sem a chave, apenas as regras YARA são atualizadas e um aviso é exibido. A chave nunca é impressa.

### `clean`: limpeza de arquivos temporários

**Sem `--apply`, o `clean` é apenas uma simulação: nada é apagado.** Ele lista o que seria removido e o espaço que seria liberado:

```
> sentinela clean
user-temp      812 arquivo(s), 1.4 GB
windows-temp   pulada (requer administrador)
crash-dumps    3 arquivo(s), 96.0 MB
browser-cache  4210 arquivo(s), 2.1 GB
    aviso: Chrome está em execução; cache ignorado
recycle-bin    57 arquivo(s), 340.2 MB
windows-update pulada (requer administrador)

Total que seria liberado: 5082 arquivo(s), 3.9 GB. Simulação: nada foi apagado; use --apply para limpar.
```

Para apagar de verdade, use `--apply`. O Sentinela lista (no stderr) cada categoria com arquivos e tamanho, avisa quando a Lixeira será esvaziada permanentemente, lista as categorias ignoradas (ex.: requer administrador) e pergunta `[s/N]` (qualquer resposta diferente de `s`, `S` ou `sim` cancela). Use `--yes` para pular a pergunta (útil em scripts):

```
> sentinela clean --category user-temp --category crash-dumps --apply
Apagar 815 arquivo(s), 1.5 GB no total? [s/N] s
```

Antes de cada exclusão o caminho é revalidado: precisa estar dentro de uma pasta permitida da categoria e não pode ser link. Arquivos em uso ou sem permissão são pulados e contabilizados. Categorias que exigem administrador são puladas com aviso quando o programa não está elevado (execute o terminal como administrador para incluí-las).

Categorias e o que cada uma apaga (use `--category <nome>`, repetível; sem ela, todas):

| Categoria | O que apaga | Admin |
| --- | --- | --- |
| `user-temp` | Arquivos de `%TEMP%` com mais de 24 horas | não |
| `windows-temp` | Arquivos de `%SystemRoot%\Temp` com mais de 24 horas | sim |
| `crash-dumps` | `%LOCALAPPDATA%\CrashDumps`; também `%SystemRoot%\Minidump` e os relatórios `ReportArchive` e `ReportQueue` do WER em `%ProgramData%\Microsoft\Windows\WER` | parcial (Minidump e WER) |
| `browser-cache` | Somente as pastas `Cache`, `Code Cache` e `GPUCache` dos perfis de Chrome, Edge e Brave, e `cache2` do Firefox. **Nunca** toca em cookies, histórico, senhas ou dados de perfil. Com o navegador aberto, a categoria é pulada com aviso | não |
| `recycle-bin` | Esvazia a Lixeira (sem janelas de confirmação do Windows) | não |
| `windows-update` | `%SystemRoot%\SoftwareDistribution\Download`. Ao aplicar, para o serviço `wuauserv`, apaga e reinicia o serviço, mesmo que a exclusão falhe | sim |
| `shader-cache` | Arquivos com mais de 24 horas em `%LOCALAPPDATA%\D3DSCache`, `NVIDIA\DXCache`, `NVIDIA\GLCache`, `AMD\DxCache`, `AMD\DxcCache` e `%USERPROFILE%\AppData\LocalLow\Intel\ShaderCache`. Os jogos recriam o cache na próxima execução | não |
| `launcher-cache` | Só o cache web dos launchers: `Steam\htmlcache`, as pastas `webcache*` da Epic, `Battle.net\BrowserCaches` e `Cache`, `Code Cache` e `GPUCache` do Discord. **Nunca** toca em `Steam\userdata`, logins, saves ou jogos. Launcher aberto é pulado com aviso | não |
| `thumbnail-cache` | Só os arquivos `thumbcache_*.db` e `iconcache_*.db` de `%LOCALAPPDATA%\Microsoft\Windows\Explorer`; os que o Explorer estiver usando são pulados | não |
| `delivery-optimization` | Cache da Otimização de Entrega em `%SystemRoot%\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache` | sim |
| `windows-logs` | `*.log` e `*.cab` de `%SystemRoot%\Logs\CBS` e `*.log` de `%SystemRoot%\Temp`, com mais de 24 horas. Nunca o log do CBS em uso | sim |

Com `--json`, o resultado segue o mesmo envelope: `summary` com `files`, `bytes`, `deleted` e `freed`; `items` com uma entrada por categoria (`id`, `plan`, `applied`, `skipped_reason`); e `errors` com os arquivos pulados.

### `tweak`: otimizações do Windows

```
sentinela tweak list [--json]
sentinela tweak apply <id>... [--yes] [--json]
sentinela tweak undo (<id>... | --all) [--yes] [--json]
```

- `list` mostra uma linha por otimização: id, estado (`aplicado`, `não aplicado`, `parcial`, `indisponível`), título e `(requer administrador)` quando bloqueada. Só lê o sistema.
- `apply` mostra a prévia de cada mudança (`descrição: atual → alvo`) e **pergunta `[s/N]` antes de aplicar**; `--yes` pula a pergunta. Com o terminal elevado, solicita um ponto de restauração antes (o Windows pode reaproveitar um ponto das últimas 24 h; se falhar, só avisa). No fim indica se é preciso reiniciar o computador ou sair e entrar de novo no Windows.
- `undo` restaura os valores originais guardados no diário, também com confirmação (`--yes` pula); `--all` desfaz tudo, da mais recente para a mais antiga. Valores alterados depois da otimização são mantidos e reportados como conflito.
- `undo <id>...` aceita qualquer id presente no diário, mesmo que a otimização tenha saído do catálogo.
- O diário fica em `%LOCALAPPDATA%\Sentinela\tweaks\journal.json`; a variável `SENTINELA_TWEAKS_JOURNAL` define outro caminho. Se o diário estiver corrompido, `apply` o guarda ao lado como `journal.json.corrompido-{segundos}` e avisa.
- O CLI sempre age na conta do Windows que está rodando o terminal: o `HKCU` e o diário são os dessa conta (um terminal elevado com outra conta altera e lista o histórico da outra conta).
- Só um processo altera as otimizações por vez; se outro estiver alterando, o item falha com "outro processo está alterando as otimizações; tente de novo."
- Códigos de saída: `0` sucesso (inclusive "nada a fazer"), `1` se algum item falhou ou houve conflito no `undo`, `2` erro global (id desconhecido, uso incorreto).
- Com `--json`, `list` devolve uma lista de `{ id, title, status, blocked, changes }`, em que `status` é `applied`, `not_applied`, `partial` ou `unavailable`; `apply` e `undo` devolvem o relatório.
- As personalizações (`taskbar-*`, `start-*`, `explorer-*`, `theme-*` e as de sistema) também aparecem no `list`. As que formam uma lista de opções (por exemplo `taskbar-align-left` e `taskbar-align-center`) são exclusivas: aplicar uma desfaz antes a outra opção do mesmo grupo que o Sentinela tiver aplicado. Quando a mudança só aparece com o Explorer reiniciado, o `apply` termina com "Reinicie o Explorer para concluir."; o CLI não reinicia o Explorer.

### `startup`: programas que iniciam com o Windows

```
sentinela startup list [--json]
sentinela startup enable <id> [--yes]
sentinela startup disable <id> [--yes]
```

- `list` mostra cada entrada das chaves `Run` (usuário, todos os usuários e 32 bits) e das pastas Inicializar (usuário e comum): id, estado (`ativado`/`desativado`), nome, origem e o comando, com `(arquivo não encontrado)`, `(requer administrador)` ou `(este é o Sentinela)` quando for o caso. Só lê o sistema.
- `enable` e `disable` **perguntam `[s/N]`** (`--yes` pula) e só alternam o estado "aprovado/desativado" de `Explorer\StartupApproved`, exatamente como o Gerenciador de Tarefas. **Nunca** apagam a entrada nem o atalho: o programa continua instalado e pode ser religado a qualquer momento (pelo Sentinela ou pelo Gerenciador de Tarefas).
- O id é `<origem>:<nome>`, como mostrado no `list`; ids com espaço vão entre aspas (`sentinela startup disable "run_hkcu:Rise Mode Controller"`).
- Entradas de todos os usuários exigem o terminal como administrador. A entrada do próprio Sentinela não é alterada aqui: use Ajustes → Inicialização na janela.
- Id desconhecido encerra com código `2` antes de qualquer pergunta. Códigos de saída: `0` sucesso (inclusive "já estava" e cancelamento), `2` erro (id desconhecido, falta de administrador, falha ao gravar).
- Com `--json`, `list` devolve `{ entries, warnings }`; cada entrada tem `id`, `name`, `command`, `source` (`run_hkcu`, `run_hklm`, `run_hklm32`, `startup_folder_user`, `startup_folder_common`), `enabled`, `file_exists` e `needs_admin`.

### `updates`: Atualizações do Windows

```
sentinela updates status [--json]
sentinela updates mode (default | security | paused --until AAAA-MM-DD | blocked) [--yes]
```

- Quatro modos, todos pelo motor de ajustes (prévia, diário, desfazer):
  - `default`: desfaz o modo ativo pelo diário e devolve exatamente o que havia antes (nunca apaga políticas que já existiam).
  - `security`: fixa a versão atual do Windows (`TargetReleaseVersion` com a versão instalada), adia as atualizações de recursos por 365 dias e as de qualidade por 4; as correções de segurança continuam chegando. É o modo recomendado.
  - `paused`: pausa tudo até a data (entre amanhã e 35 dias); depois o Windows volta a atualizar sozinho.
  - `blocked`: desliga as atualizações automáticas por política, desativa `wuauserv`, `UsoSvc` e `BITS` e as tarefas agendadas de atualização. O Windows fica **sem correções de segurança**; a Microsoft Store e alguns instaladores falham. Tarefas protegidas que negam acesso são puladas e avisadas.
- `status` mostra o modo, há quantos dias foi aplicado e, em "Parcial", o que o Windows desfez sozinho (o Sentinela não reaplica por conta própria). Só lê o sistema.
- `mode` exige o terminal como administrador, mostra a prévia e **pergunta `[s/N]`** (`--yes` pula). Erros de uso (`paused` sem `--until`, `--until` em outro modo, modo desconhecido) saem com código `2` antes de qualquer leitura.
- Em `blocked` e `paused`, a janela do Sentinela atualiza as definições do Defender uma vez por dia direto da Microsoft (`MpCmdRun -SignatureUpdate -MMPC`, que não passa pelo Windows Update).
- Códigos de saída: `0` sucesso (inclusive cancelamento), `1` se o modo não pôde ser aplicado por inteiro, `2` erro de uso ou falta de administrador.

### `games`: GPU de alto desempenho por jogo

```
sentinela games list [--json]
sentinela games gpu (on | off) <id> [--yes]
```

- `list` mostra os jogos da Steam (todas as bibliotecas), da Epic, da Riot e os adicionados na janela ("Adicionar jogo…"), um por executável, com `GPU+` quando a preferência de alto desempenho está ligada. Em jogos Unreal, o executável é o `*-Win64-Shipping.exe`, o processo que de fato usa a placa de vídeo. Só lê o sistema.
- `gpu on` grava a mesma preferência que Configurações → Sistema → Tela → Gráficos ("Alto desempenho"); `gpu off` volta ao que era antes, inclusive para um jogo que já foi desinstalado. Pergunta `[s/N]` (`--yes` pula); não exige administrador.
- Id desconhecido encerra com código `2`. Com `--json`, `list` devolve `{ id, name, source, exe, high_performance }`.

### `net`: rede (só mede)

```
sentinela net test [<destino>...] [--count <N>] [--json]
sentinela net trace <destino> [--json]
sentinela net bufferbloat [--json]
sentinela net dns [--json]
sentinela net history [--json]
sentinela net routes [--json]
sentinela net compare <destino> [--route <id>]... [--rounds <N>] [--json]
sentinela net route <id> (on | off)
```

- `test` mede latência média, *jitter* e perda (20 sondas por destino; `--count` muda). Sem destino, mede o roteador, a Cloudflare e os destinos acrescentados na página Rede. Destino com porta (`servidor:27015`) é medido pelo tempo de conexão TCP, para servidores que não respondem a ping.
- `trace` mostra a rota salto a salto, separando a rede de casa do provedor (CGNAT conta como provedor), e marca o salto em que o atraso começa quando o aumento se mantém até o fim da rota.
- `bufferbloat` mede o atraso até o roteador parado e de novo com um download da Cloudflare ocupando a conexão por cerca de 15 segundos (baixa algumas centenas de MB numa conexão rápida). "Alto" pede SQM/QoS no roteador.
- `dns` compara o DNS em uso com Cloudflare, Google, Quad9, AdGuard e OpenDNS e diz se vale trocar; **não muda nada**. Trocar o DNS (e os ajustes de adaptador) fica na página Rede, com prévia, e se desfaz pelo Histórico. O DNS não muda o ping dentro do jogo.
- Cada medição entra no histórico (o mesmo da página Rede, últimas 200). A rota até o servidor do jogo é decidida pelo provedor e pelas redes no caminho: o Sentinela mede e ajusta só o que é do computador.
- `routes` lista o Cloudflare WARP (`warp`), os túneis WireGuard importados na página Rede (`wg:nome`) e serviços pagos instalados (ExitLag, NoPing: só detectados, o Sentinela não liga nem desliga).
- `compare` mede a conexão atual e cada rota em rodadas intercaladas (3 por padrão) e recomenda uma só quando a mediana dela fica pelo menos 5 ms menor **e** a diferença passa da soma dos *jitters*, sem perder mais pacotes. Cada rota volta ao estado em que estava. Com o túnel ligado, o tráfego passa pelo servidor daquele serviço; alguns jogos e anti-cheats bloqueiam VPN.
- `route` liga ou desliga uma rota. O WARP precisa ter sido aberto uma vez (o Sentinela não registra conta nem aceita termos); túnel WireGuard exige o terminal como administrador.
- `warp-register --accept-tos` ativa o aplicativo Cloudflare WARP (conta gratuita) sem abrir a janela dele.
- `warp-tunnel --accept-tos [--name warp] [--on]` registra um dispositivo gratuito no WARP, gera o perfil WireGuard (como o `wgcf`) e importa como túnel `wg:warp`; `--on` liga em seguida (administrador). A chave privada nasce no computador e fica só em `%LOCALAPPDATA%\Sentinela\wireguard`. Os dois comandos aceitam em seu nome os Termos de Serviço da Cloudflare, por isso exigem `--accept-tos` (na janela, um diálogo pergunta).
- Destino inválido ou que não resolve, ou rota desconhecida, encerra com código `2`.

### `apps`: aplicativos via winget

```
sentinela apps list [--category <id>] [--json]
sentinela apps install <id>... [--yes]
sentinela apps upgrade (<id>... | --all) [--yes]
```

- Só aplicativos do catálogo embutido, e sempre instalados pelo **winget** (nunca por download direto). Exige o winget 1.6 ou mais novo.
- `list` mostra id, nome, categoria e estado (`instalado`, `atualização disponível`, `não instalado` ou `?` quando o winget não responde). `--category` filtra por categoria (`navegadores`, `comunicacao`, `jogos`, `ferramentas_jogos`, `multimidia`, `utilitarios`, `desenvolvimento`). Só lê o sistema; sem winget, lista tudo com `?` e sai com `0`.
- `install` e `upgrade` listam o que vão fazer, mostram o aviso "Ao continuar, você aceita os termos de licença de cada aplicativo." e **perguntam `[s/N]`**; `--yes` pula a pergunta (o aviso continua aparecendo). Um aplicativo por vez, com uma linha por item (`[1/3] Steam… instalado.`). Prévia, aviso e pergunta saem no stderr.
- `upgrade --all` atualiza todo aplicativo do catálogo com atualização disponível; se não houver nenhum, imprime `Nada para atualizar.`.
- Ctrl+C para depois do aplicativo atual: o winget em andamento nunca é interrompido à força, e os itens restantes não são processados.
- Ids repetidos são tratados uma vez. Id desconhecido encerra com código `2` antes de qualquer pergunta ou chamada ao winget.
- Códigos de saída: `0` sucesso (inclusive "nada a fazer" e cancelamento), `1` se algum item falhou ou a fila foi interrompida, `2` erro global (id desconhecido, winget indisponível, uso incorreto).
- Com `--json`, `list` devolve uma lista de `{ id, name, category, winget, state }`, em que `state` é `installed`, `update_available`, `not_installed` ou `null` (desconhecido).
- A variável `SENTINELA_WINGET` (caminho do executável) existe **só para testes**; apontando para algo inexistente, simula o winget ausente.

### `find-large`: maiores arquivos

```
> sentinela find-large D:\ --top 3 --min-size 500
   4.2 GB  D:\VMs\win11.vhdx
   1.8 GB  D:\Videos\viagem.mkv
 612.0 MB  D:\Instaladores\jogo.iso
```

Padrões: `--top 20`, `--min-size 0` (em MB). Somente lista; nunca apaga.

### `find-dupes`: arquivos duplicados

```
> sentinela find-dupes D:\Fotos --min-size 100
2 x 9f2c1a7b03de (3.1 MB cada)
    D:\Fotos\2024\img_001.jpg
    D:\Fotos\backup\img_001.jpg

Espaço desperdiçado: 3.1 MB
```

Agrupa por tamanho, depois pelo hash dos primeiros 64 KB e por fim pelo SHA-256 completo. Padrão `--min-size 1` (em KB). Somente relata grupos; **nunca apaga**.

### `info`

```
> sentinela info
sentinela 0.1.0
Banco de assinaturas: C:\Users\eu\AppData\Local\Sentinela\db
  malwarebazaar-recent — atualizado em 2026-10-22T12:00:00Z (48213 itens)
  yara-forge-core — atualizado em 2026-10-22T12:00:00Z (640 itens)
Executando como administrador: não
```

Sem banco local: `banco não inicializado — rode `sentinela update``.

## Como compilar

Requer o [Rust](https://rustup.rs) estável e, para a janela, o [Node.js](https://nodejs.org) 22.

Linha de comando e núcleo:

```bash
cargo build --release -p sentinela
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

Janela e instalador (em `app/`; o `dist` do frontend precisa existir para compilar o `sentinela-gui` com `cargo`):

```bash
cd app
npm ci
npm test                # testes do frontend
npm run build           # compila o frontend
npm run tauri dev       # janela em modo de desenvolvimento
npm run tauri build     # gera o instalador NSIS
```

O `npm run tauri build` compila o frontend e o CLI em release, copia o `sentinela.exe` para `app/src-tauri/bin/` e empacota tudo em `target/release/bundle/nsis/Sentinela_<versão>_x64-setup.exe`. O roteiro de testes manuais está em [docs/testes-manuais-fase-1.md](docs/testes-manuais-fase-1.md).

Ao publicar uma tag `v*`, o GitHub Actions compila no Windows e cria a release com o instalador e o `sentinela.exe` solto.

## Licença

MIT, veja [LICENSE](LICENSE).

## Links e arquivos somente na nuvem

O `scan` e o `find` não seguem links simbólicos nem junções; cada um é listado como erro (`link simbólico/junção ignorado`), inclusive quando a pasta raiz é um link. Arquivos do OneDrive (Files On-Demand) que ainda não foram baixados não são lidos, para não disparar o download: aparecem como erro `arquivo somente na nuvem (não baixado); não verificado`. Já a limpeza (`clean`) é mais estrita e recusa qualquer ponto de reanálise. A idade mínima de 24 h dos temporários considera a mais recente entre a data de modificação e a de criação.
