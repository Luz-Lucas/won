# Sentinela 2 — Interface gráfica, otimização para jogos e central de aplicativos

Data: 2026-11-16
Status: aguardando revisão
Referência: [ChrisTitusTech/winutil](https://github.com/ChrisTitusTech/winutil)

## 1. Objetivo

Transformar o Sentinela (hoje um CLI de antivírus + limpeza) em um aplicativo de janela para Windows 11 que, **sem o usuário abrir um terminal**, permite:

1. **Otimizar o Windows 11 para jogos**, com foco em FPS competitivo (menos latência, menos processos em segundo plano, frame time mais estável).
2. **Limpar arquivos**, cache e temporários (o que o `clean` já faz, mais caches específicos de jogos).
3. **Instalar aplicativos de uso geral** a partir de um catálogo, com um clique.
4. **Personalizar o Windows** (barra de tarefas, menu Iniciar, Explorador, aparência) com o alcance do Windhawk, **feito pelo próprio app, sem depender do Windhawk instalado**.
   O alvo é **personalização máxima, no nível de um desktop Linux**: barra própria, janelas lado a lado automáticas (*tiling*), áreas de trabalho, lançador, atalhos, bordas e transparência, e temas completos que se exportam num arquivo (§9.3–9.5). O papel de parede é o **Wallpaper Engine**; tudo precisa funcionar com ele e tirar proveito dele (§9.4).
5. **Ser o antivírus sob demanda do computador inteiro** (§15): verificação rápida das áreas críticas e dos arquivos de maior risco, verificação completa que lê tudo e avalia riscos, e quarentena quando necessário.
6. Usar pela janela o resto que já existe: atualização de assinaturas, arquivos grandes e duplicados.

Critérios de sucesso:

- Todas as funções são alcançáveis pela janela; nenhuma exige digitar comando.
- Toda otimização aplicada pode ser **desfeita** pelo próprio app, voltando ao valor que estava antes na máquina (não a um "padrão" presumido).
- Nada é aplicado sem o usuário ver antes o que vai mudar (mesma filosofia do `clean` sem `--apply`).
- O CLI atual continua funcionando igual, com os mesmos testes passando.

## 2. Não objetivos e anti-objetivos

**Fora do escopo desta expansão:** proteção em tempo real, MicroWin/ISO customizada, remoção de Edge/OneDrive, debloat amplo de Appx, overclock, troca de drivers, suporte a Windows 10 (pode funcionar, não é testado), outros idiomas além de pt-BR, motor de papel de parede próprio (é o Wallpaper Engine), trocar o `explorer.exe` por outro shell, versão portátil do app, e qualquer sistema que não seja Windows (o CI em Linux e o código de compatibilidade do design original saem; ver §12).

**Anti-objetivos (fariam a expansão fracassar mesmo funcionando):**

- **Enfraquecer a segurança para ganhar FPS.** O Sentinela é um produto de segurança: nunca desativa Defender, Windows Update, firewall, UAC, mitigações de CPU nem integridade de memória. *(Decisão minha; ver §12.)* Duas exceções, pedidas pelo usuário e sem relação com FPS, ambas desligadas por padrão e fora de qualquer perfil: a verificação de assinatura de **temas visuais**, só para quem ligar os temas `.msstyles` (§9.6), e o **bloqueio de atualizações do Windows** (§6.5).
- **Otimização placebo vendida como ganho.** Cada tweak declara o nível de evidência; os controversos ficam em "Avançado", desmarcados.
- **Scripts arbitrários.** O winutil executa blocos PowerShell (`InvokeScript`) vindos de JSON. Aqui só existem ações tipadas e testáveis.
- **Baixar instaladores por URL direta.** Aplicativos só via winget, que valida o hash do instalador.
- **Injetar código fora do shell do Windows ou código que não seja do Sentinela.** Os mods (§9.2) entram só nos processos do shell, nunca em jogos, navegadores ou outros apps, e são todos embutidos e revisados — o app não compila nem carrega mods de terceiros.

## 3. O que existe hoje e o que a mudança toca

O repositório é um crate Rust único (~7.500 linhas) com `lib.rs` expondo módulos bem separados: `walker`, `scan/*`, `signatures`, `updater`, `clean/*` (trait `Category`, `Env` com raiz falsa para testes, `plan` → `apply`), `finder`, `quarantine`, `full`, `report`, `progress`, `cli`.

Pontos que facilitam a expansão:

- `clean::run_clean`, `full::run_full`, `finder` e `scan` já devolvem relatórios `serde` — servem direto de resposta para a interface.
- O padrão "simular, mostrar, confirmar, aplicar, revalidar" do `clean` é o modelo para os tweaks.
- `clean::platform::is_elevated` e o controle de serviço (`run_with_service`) são reaproveitáveis.

Pontos que precisam mudar antes da interface:

- *(Corrigido em 2026-11-16, ao ler o código para o plano de implementação.)* Progresso e cancelamento da varredura **já são injetados**: `scan::Progress` (trait) e `cancel: &AtomicBool` em `run_scan_with_progress` e `run_full`; o `ctrlc` e a barra do terminal vivem só no `cli.rs`/`progress.rs`. Falta apenas uma implementação de `Progress` que emita eventos para a janela, e cancelamento no `finder`, que hoje não tem.
- Vários relatórios serializam `PathBuf` direto; um caminho com Unicode inválido faz a serialização inteira falhar. Precisa de serialização tolerante antes de mandar relatórios para a janela.
- A confirmação `[s/N]` vive no `cli.rs`; a regra de negócio já está fora dele, basta não depender do stdin.

## 4. Abordagens consideradas

| | Abordagem | A favor | Contra |
|---|---|---|---|
| **A** | **Tauri 2 + React/TypeScript** chamando o núcleo Rust em processo | Reusa 100% do núcleo sem serialização extra; visual moderno e livre (importante para um app "gamer"); WebView2 já vem no Windows 11; executável ~10–15 MB | Adiciona Node ao build; duas linguagens |
| B | egui/eframe (Rust puro) | Um só toolchain; exe único e leve | Visual utilitário, difícil chegar a um acabamento de produto; listas grandes e ícones dão mais trabalho |
| C | WPF + PowerShell (como o winutil) | Referência pronta para copiar | Descarta o núcleo Rust ou vira um invólucro de subprocesso; herda o modelo de scripts que é anti-objetivo |
| D | GUI em .NET/WinUI chamando `sentinela.exe --json` | UI nativa | Dois executáveis, progresso por parsing de stdout, mais superfície de erro |

**Escolha: A.** O núcleo já é uma biblioteca; o Tauri o chama como função, e a parte visual fica na tecnologia em que é mais barato fazer bonito. B é a alternativa se a prioridade for exe único sem WebView.

## 5. Arquitetura

```
sentinela/
  Cargo.toml                    # workspace
  crates/
    core/                       # o lib atual + módulos novos (tweaks, apps, mods)
    cli/                        # bin `sentinela` (cli.rs atual)
    shell-mods/                 # cdylib `sentinela_shell.dll`: os mods, carregada no shell (§9.2)
    agent/                      # bin `sentinela-agent`: residente mínimo que carrega a DLL (§9.2)
    desktop/                    # bin `sentinela-desktop`: barra, tiling, lançador, atalhos, bordas (§9.3)
  app/
    src-tauri/                  # bin `sentinela-gui`: comandos Tauri finos sobre o core
    src/                        # React + TypeScript + Vite
  catalog/
    tweaks.json                 # embutido com include_str!
    apps.json                   # embutido com include_str!
```

Regras:

- `src-tauri` não contém regra de negócio: cada comando valida entrada, chama o `core` numa thread de trabalho e emite eventos (`progress`, `done`, `error`). Tudo que a janela faz também é possível pelo CLI.
- Catálogos são dados embutidos no binário e validados por teste (ids únicos, chaves de registro dentro de uma lista de raízes permitidas, ids winget bem formados). Sem catálogo remoto na primeira versão.
- Estado do app em `%LOCALAPPDATA%\Sentinela\` (já usado): `tweaks\journal.json`, `settings.json`.

### 5.1 Privilégios

O app abre **sem elevação**. Itens que exigem administrador aparecem com cadeado e um botão "Reiniciar como administrador" (um UAC, relança o processo). Motivo: limpeza de usuário, catálogo e varredura de pastas pessoais não precisam de admin, e o comportamento atual do `clean` ("pulada: requer administrador") já segue esse modelo. Tweaks em `HKCU` são gravados sempre no usuário que abriu o app; se o UAC elevar para **outra** conta, os tweaks de `HKCU` ficam bloqueados com aviso.

## 6. Motor de otimizações (`core::tweaks`)

### 6.1 Modelo

```rust
struct Tweak {
    id: String, title: String, description: String,
    group: Group,              // Jogos, Rede, Energia, SegundoPlano, Privacidade, Visual, Personalizacao
    evidence: Evidence,        // Comprovado | Situacional | Controverso
    needs_admin: bool, needs_reboot: bool,
    laptop_warning: bool,
    actions: Vec<Action>,
}
enum Action {
    Registry { hive, path, name, kind, value },      // valor ausente = apagar
    Service  { name, startup },                       // Automatic|Manual|Disabled
    PowerPlan { scheme },                             // via API de energia, sem powercfg.exe
    ScheduledTask { path, enabled },
    GpuPreference { exe, high_performance },          // por jogo
}
```

Cada `Action` implementa três operações contra um `trait SystemBackend`: `read()` (estado atual), `apply()`, `restore(snapshot)`. Há um backend real (Windows) e um em memória para testes — o mesmo truque do `Env` com raiz falsa.

### 6.2 Fluxo

1. **Detectar:** ao abrir a página, cada tweak mostra Aplicado / Não aplicado / Parcial, lido do sistema.
2. **Prévia:** ao selecionar, o usuário vê a lista exata de mudanças (chave, valor atual → valor novo).
3. **Aplicar:** antes da primeira aplicação da sessão, cria um ponto de restauração do Windows (se a Proteção do Sistema estiver ligada; senão avisa e segue). Para cada ação, grava no diário o valor anterior **antes** de escrever.
4. **Desfazer:** por tweak ou "Desfazer tudo", restaurando do diário. Se o valor atual não é mais o que o Sentinela gravou (o usuário ou o Windows mudou), avisa em vez de sobrescrever.

### 6.3 Catálogo inicial

**Comprovado (pré-selecionado no perfil "Recomendado"):**

| Tweak | O que muda |
|---|---|
| Modo de Jogo ligado | `HKCU\Software\Microsoft\GameBar` |
| Gravação em segundo plano (Game DVR) desligada | `HKCU\System\GameConfigStore`, política `GameDVR` |
| Agendamento de GPU acelerado por hardware | `GraphicsDrivers\HwSchMode` (reinício) |
| Otimizações para jogos em janela + VRR | `DirectX\UserGlobalSettings` |
| Aceleração do mouse desligada | `HKCU\Control Panel\Mouse` |
| Plano de energia Alto/Máximo Desempenho | aviso em notebook |
| GPU de alto desempenho por jogo | lista de jogos detectados (Steam, Epic, Riot, Battle.net) |
| Apps em segundo plano e inicialização | gerenciador de itens de inicialização (liga/desliga, nunca apaga) |
| Efeitos visuais e transparência reduzidos | `VisualEffects`, `Themes\Personalize` |
| Telemetria e histórico de atividades reduzidos | serviços `DiagTrack` etc. para Manual, políticas |
| Otimização de Entrega (P2P de updates) desligada | política `DeliveryOptimization` |

**Situacional / Controverso (aba "Avançado", desmarcado, com explicação):**

| Tweak | Observação |
|---|---|
| Otimizações de tela cheia desligadas globalmente | Ajuda em alguns jogos antigos, piora em outros |
| `SystemResponsiveness` / `NetworkThrottlingIndex` (MMCSS) | Ganho medido é pequeno e depende do jogo |
| Algoritmo de Nagle desligado por adaptador | Só afeta jogos TCP |
| Resolução global do timer | Aumenta consumo; notebook não |
| Serviços extras para Manual (lista do winutil, revisada) | `SysMain`, busca, etc. |
| Indexação de busca desligada | Piora a busca do Windows |

**Perfis:** Recomendado · FPS competitivo (Recomendado + Situacionais) · Personalizado. Equivalente ao `preset.json` do winutil.

### 6.4 Modo Jogo sob demanda (fase posterior)

Botão "Iniciar sessão de jogo": troca o plano de energia, pausa os processos que o usuário marcou (launchers, sincronização, RGB) e **reverte tudo ao encerrar**. Nada persistente.

### 6.5 Atualizações do Windows

Pedido do usuário: o app bloqueia as atualizações do Windows. Fica numa seção própria ("Atualizações do Windows", exige administrador), com um seletor de quatro modos em vez de um interruptor, no modelo do winutil:

| Modo | O que faz |
|---|---|
| **Padrão** | Devolve tudo ao que estava antes (pelo diário) |
| **Só segurança** | Fixa a versão atual do Windows (não instala versões novas como 25H2→26H2), adia atualizações de recursos ao máximo e as de qualidade por alguns dias; correções de segurança continuam chegando |
| **Pausado até…** | Pausa tudo até a data escolhida; volta sozinho depois |
| **Bloqueado** | Desliga as atualizações automáticas por política, põe os serviços de atualização (`wuauserv`, `UsoSvc`) como desativados e desliga as tarefas agendadas do orquestrador de atualizações |

Chaves, políticas e tarefas exatas de cada modo são confirmadas na implementação, build a build; o Windows tem mecanismos que religam o Windows Update sozinho, então o modo Bloqueado é reconferido a cada abertura do app e mostrado como "Parcial" se algo voltou.

Regras:

- Passa pelo motor de tweaks: prévia, diário, desfazer. É o único tweak autorizado a tocar nos serviços de atualização (§13).
- **Defender continua atualizado.** As definições do Defender chegam pelo Windows Update; com tudo bloqueado, o Sentinela passa a atualizá-las ele mesmo uma vez por dia, direto da Microsoft (`MpCmdRun -SignatureUpdate`; a confirmar que funciona com o serviço desativado — se não funcionar, o modo Bloqueado mantém o serviço em Manual e bloqueia só por política).
- **Aviso permanente:** enquanto estiver em Bloqueado ou Pausado, a página Início mostra há quantos dias o Windows não recebe correções. O botão **"Atualizar agora"** libera, abre o Windows Update e volta ao modo escolhido quando a instalação termina.
- **Efeitos colaterais avisados ao ligar:** a Microsoft Store e alguns instaladores (inclusive pacotes `msstore` do winget, .NET e drivers) falham com o serviço desativado; a página Aplicativos detecta o bloqueio e oferece liberar temporariamente.
- A categoria de limpeza `windows-update` hoje para e **reinicia** o `wuauserv`; passa a devolver o serviço ao estado em que o encontrou, para não desfazer o bloqueio.
- Ligação com a personalização: mods (§9.2) e temas visuais (§9.6) quebram quando o build muda; a página deles sugere o modo "Só segurança" para manter o build estável sem ficar sem correções.

### 6.6 Rede e conexão

Pedido do usuário (2026-11-17): melhorar as rotas de internet e otimizar a conexão.

**Limite honesto:** a rota que um pacote faz até o servidor do jogo é decidida pelo provedor e pelas redes no caminho, não pelo Windows. Programas como ExitLag e NoPing mudam a rota porque mandam o tráfego por **servidores próprios** espalhados pelo mundo; um app local sozinho não tem como fazer isso. O Sentinela, portanto, faz três coisas: **mede**, **ajusta o que é local** e **testa rotas alternativas que o usuário pode usar**, sempre mostrando o antes e depois. Nada é vendido como ganho sem medição.

**1. Medir (página Rede)**

- Teste de latência, *jitter* e perda de pacotes até destinos escolhidos: servidores de jogos conhecidos por região (lista embutida e editável: Valve/CS2, Riot, Battle.net, Epic/Fortnite, EA), o roteador e um DNS público. ICMP e, onde o servidor não responde a ping, conexão TCP na porta do serviço.
- Rastreamento de rota (`traceroute` próprio, ICMP) com a latência por salto, mostrando **onde** o atraso começa: na rede de casa/Wi‑Fi, no provedor ou depois dele.
- Teste de *bufferbloat*: latência ao roteador parado × durante um download de teste. Diagnóstico em linguagem simples ("o atraso sobe 180 ms quando a rede está ocupada: ative o SQM/QoS no roteador").
- Histórico dos testes, para comparar antes e depois de cada mudança.

**2. Ajustar o que é local** (motor de tweaks do §6: prévia, diário, desfazer; grupo `Rede`)

| Ajuste | Evidência |
|---|---|
| **DNS mais rápido:** testa os DNS públicos (Cloudflare, Google, Quad9, AdGuard, OpenDNS e o do provedor) a partir da máquina e aplica o mais rápido por adaptador, IPv4 e IPv6; opção de DNS sobre HTTPS do próprio Windows | Comprovado (afeta resolução de nomes, não o ping em jogo; dito na tela) |
| **Adaptador de rede:** desligar economia de energia do adaptador, *Energy-Efficient Ethernet* e *Green Ethernet*; ajustar moderação de interrupção | Situacional |
| **Wi‑Fi:** avisar quando o jogo está em Wi‑Fi, mostrar sinal e canal, desligar a varredura em segundo plano enquanto um jogo estiver aberto | Situacional |
| **Banda em segundo plano:** monitor de uso de rede por processo; durante o Modo Jogo (§6.4), pausa a Otimização de Entrega, o Windows Update (sem mudar o modo do §6.5) e os apps de sincronização que o usuário marcar | Comprovado |
| **QoS por jogo:** política do Windows que marca o tráfego do executável do jogo com prioridade (DSCP 46) | Controverso: só ajuda se o roteador respeitar a marcação; fica em "Avançado" |
| Algoritmo de Nagle, `NetworkThrottlingIndex`, autoajuste de TCP | Já listados no §6.3 "Avançado", com o mesmo aviso |

Não entram: "aceleradores de internet" que mudam dezenas de chaves TCP sem medição, MTU forçado, e qualquer coisa que desligue IPv6 globalmente.

**3. Testar rotas alternativas**

- **Comparador de rota:** para um destino de jogo, mede latência e perda pela conexão normal e por cada alternativa instalada, lado a lado, com repetições suficientes para ver diferença real. Recomenda a alternativa só quando a melhora passa da variação medida.
- **Alternativas suportadas:**
  - **Cloudflare WARP** (gratuito; instalado pelo catálogo do §8, `Cloudflare.Warp`): em parte das conexões ele leva o tráfego por uma rota melhor até a região do servidor, em outras piora — por isso só com o comparador. O Sentinela liga e desliga o WARP pela linha de comando dele (`warp-cli`), sem guardar contas.
  - **Túnel WireGuard do próprio usuário** (um servidor VPS perto do jogo): importa o arquivo `.conf`, liga e desliga pelo WireGuard oficial (`WireGuard.WireGuard` no winget) e entra no comparador.
  - **Serviços pagos de rota para jogos** (ExitLag, NoPing e similares): o Sentinela não reimplementa nem se integra a eles; detecta se estão instalados e inclui a conexão deles no comparador quando o usuário os liga.
- **Por jogo:** "usar rota X neste jogo" liga a alternativa ao abrir o jogo e desliga ao fechar, pela mesma detecção de jogos do §9.2. Túnel dividido (só o jogo pelo túnel) quando a alternativa permite.
- Antes de ligar qualquer túnel, a janela avisa que o tráfego passa pelo servidor daquele serviço, e que alguns jogos e anti-cheats bloqueiam VPN.

## 7. Limpeza

A página lista as categorias com checkbox, roda a simulação automaticamente e mostra arquivos/tamanho por categoria; "Limpar" abre a confirmação com o total e o aviso da Lixeira. É o `run_clean` atual.

Categorias novas (mesmo trait `Category`, mesmas garantias e testes obrigatórios):

| Categoria | Pastas |
|---|---|
| `shader-cache` | `%LOCALAPPDATA%\D3DSCache`, NVIDIA `DXCache`/`GLCache`, AMD `DxCache`/`DxcCache`, Intel `ShaderCache` |
| `launcher-cache` | Caches web de Steam (`htmlcache`), Epic (`webcache*`), Battle.net, Discord (`Cache`, `Code Cache`, `GPUCache`). Pulada com o app aberto, como o cache de navegador. Nunca toca em jogos, saves ou login |
| `thumbnail-cache` | `%LOCALAPPDATA%\Microsoft\Windows\Explorer\thumbcache_*.db` |
| `delivery-optimization` | Cache da Otimização de Entrega (admin) |
| `windows-logs` | `%SystemRoot%\Logs\CBS`, `*.log` antigos em `%SystemRoot%\Temp` (admin) |

Na mesma área: **Disco** (`find-large` e `find-dupes` com "abrir pasta"; continuam sem apagar nada).

## 8. Aplicativos

- `catalog/apps.json` no formato do `applications.json` do winutil reduzido: `id`, `name`, `description`, `category`, `winget`, `link`, `foss`.
- Catálogo inicial enxuto (~60 itens) em: Navegadores, Comunicação, **Jogos** (Steam, Epic, GOG, Battle.net, EA, Ubisoft, Discord), **Ferramentas para jogos** (MSI Afterburner, HWiNFO, CapFrameX, DDU, NVIDIA App, AMD Adrenalin), Multimídia, Utilitários, Desenvolvimento.
- Tela: busca, filtro por categoria, cartões com checkbox, selo "Instalado" / "Atualização disponível", fila de instalação com progresso por item.
- Execução: `winget install --id <id> -e --silent --accept-package-agreements --accept-source-agreements --disable-interactivity`, como subprocesso oculto atrás de um `trait PackageManager` (falso nos testes). Estado instalado via `winget list` em JSON/saída tabulada, com cache por sessão.
- Sem winget: tela explica e abre a página do "Instalador de Aplicativo" na Microsoft Store.
- "Atualizar tudo" (`winget upgrade`) como ação secundária.

## 9. Personalização do Windows

Pedido do usuário: oferecer personalizações como as do [Windhawk](https://windhawk.net), **direto no app, sem precisar do Windhawk**. O Windhawk funciona **injetando código** no `explorer.exe` e em outros processos; é isso que permite mudar coisas que o Windows não expõe (altura da barra de tarefas, estilo do menu Iniciar, relógio). A página tem dois níveis, ambos feitos pelo Sentinela:

### 9.1 Nativas (motor de tweaks, sem injeção)

Mesmas garantias do §6: prévia, diário, desfazer. Grupo `Personalizacao` no catálogo, sem perfil pré-selecionado — cada item é um interruptor ou uma lista de opções.

| Área | Itens |
|---|---|
| Barra de tarefas | Alinhamento (esquerda/centro), ícones de Busca, Visão de Tarefas, Widgets e Copilot, "Finalizar tarefa" no clique direito, segundos no relógio, agrupar botões |
| Menu Iniciar | Recomendações, busca do Bing, mais fixados × mais recomendados |
| Explorador | Menu de contexto clássico, extensões e arquivos ocultos, abrir em "Este Computador", ocultar Início/Galeria, modo compacto |
| Aparência | Tema escuro/claro (apps e sistema separados), cor de destaque, transparência, animações |
| Sistema | Tela de bloqueio, desfoque no logon, NumLock na inicialização, Teclas de Aderência, encaixe de janelas |

Itens que pedem reinício do Explorer oferecem o botão "Reiniciar Explorer" em vez de fazê-lo sozinhos.

### 9.2 Profundas (motor próprio de mods)

Decisão do usuário: nada de depender do Windhawk. O Sentinela ganha um motor próprio, pequeno e fechado, para o que o registro não alcança. É a parte mais cara e mais frágil da expansão (ver §14); por isso é a última a entrar e tem as próprias travas.

**Peças**

- **`sentinela_shell.dll`** (`crates/shell-mods`, Rust `cdylib`): contém todos os mods. Cada mod é um módulo com `enable(settings)` / `disable()` e instala *hooks* em funções do shell.
- **`sentinela-agent.exe`** (`crates/agent`): processo residente mínimo, sem janela, registrado para iniciar no logon **somente enquanto houver algum mod ligado**. Carrega a DLL nos processos do shell, recarrega quando o Explorer reinicia e fala com a janela por *named pipe* (ligar/desligar mod, mudar ajuste, estado). Roda sem elevação: o shell é do mesmo usuário.
- **`core::mods`**: catálogo dos mods embutidos, ajustes tipados por mod (`%LOCALAPPDATA%\Sentinela\mods\settings.json`), e o protocolo com o agente. A janela e o CLI (`sentinela mods list/on/off`) usam o mesmo módulo.

**Como os mods alcançam o shell** (técnicas a confirmar na implementação, build a build)

- *Hooks* de função em `explorer.exe` e nas DLLs da barra (`Taskbar.dll`, `Taskbar.View.dll`), com uma biblioteca de *inline hook* (candidata: crate `retour`).
- Funções não exportadas são localizadas por **símbolos públicos da Microsoft**: o agente baixa o PDB do build instalado do servidor de símbolos oficial, resolve os endereços e guarda em cache por versão de arquivo. Sem símbolo, o mod não liga.
- Estilo da barra e do menu Iniciar (XAML) mexendo na árvore visual pela API de diagnóstico de XAML, dentro de `explorer.exe` e `StartMenuExperienceHost.exe`.

**Mods embutidos, em ordem de entrega**

| # | Mod | Ajustes |
|---|---|---|
| 1 | Relógio da barra | Formato de hora/data, segundos, segunda linha, texto livre |
| 2 | Rolagem na barra controla o volume | Passo, só sobre a área do relógio ou barra inteira |
| 3 | Clique do meio fecha a janela na barra | Fechar / nova instância |
| 4 | Altura da barra e tamanho dos ícones | Altura, tamanho, espaçamento |
| 5 | Rótulos e desagrupamento dos botões | Largura máxima, quando mostrar |
| 6 | Estilo da barra | Predefinições (translúcida, desfocada, cantos) + cores |
| 7 | Estilo do menu Iniciar | Predefinições (compacto, sem recomendados, lista de apps aberta) |

1–3 são *hooks* simples e validam o motor; 4–5 dependem de símbolos; 6–7 dependem de XAML e ficam por último.

**Travas**

- **Lista fechada de processos**, fixa no código e coberta por teste: `explorer.exe`, `StartMenuExperienceHost.exe`, `ShellExperienceHost.exe`. O agente recusa qualquer outro alvo; não existe opção para ampliar.
- **Falha fechada por build:** cada mod declara os builds do Windows em que foi testado. Build desconhecido → mod fica desligado, com a opção "tentar mesmo assim (experimental)". Símbolo ou elemento não encontrado → o mod não se aplica; nunca escreve em endereço presumido.
- **Modo seguro:** se o Explorer cair duas vezes em até 60 s depois de carregar a DLL, o agente desliga todos os mods, não recarrega e avisa na próxima abertura do app. `sentinela mods off` no CLI faz o mesmo à mão.
- **Tudo reversível ao vivo:** desligar um mod remove os *hooks* e devolve o estado sem logoff; "Desligar todos" descarrega a DLL.
- **Integridade:** DLL e agente ficam na pasta de instalação (gravável só por administrador); o agente confere o hash da DLL antes de carregá-la.
- **Parada em jogos (sempre ativa, sem opção de desligar):** enquanto um jogo estiver aberto, os mods ficam parados — o agente remove os *hooks*, descarrega a DLL do shell e só recarrega quando o último jogo fechar. A barra volta ao visual padrão do Windows durante a partida. O agente considera "jogo aberto" quando vê qualquer um destes:
  - um executável das bibliotecas detectadas (Steam, Epic, Riot, Battle.net, EA, Ubisoft, Xbox; mesma lista do §6.3);
  - um executável que o usuário adicionou à mão em Ajustes;
  - um processo de anti-cheat conhecido subindo (EasyAntiCheat, BattlEye, cliente do Vanguard, FACEIT), que cobre jogos fora da lista.

  Limites conhecidos: a detecção acontece logo **depois** de o processo aparecer, não antes; e anti-cheats que ficam ativos desde o boot (Vanguard) já viram o Explorer com mods antes de o jogo abrir. Para esse caso há a opção "Não carregar mods enquanto o serviço do Vanguard estiver ativo", desligada por padrão.
- **Código próprio:** os mods são reimplementados a partir do comportamento, não copiados. O Windhawk é GPLv3 e cada mod dele tem licença própria; aproveitar código de lá só depois de conferir compatibilidade com a licença do repositório.

### 9.3 Ambiente de desktop (nível Linux)

Pedido do usuário: personalização máxima, como num desktop Linux. No Linux isso vem de peças separadas (gerenciador de janelas, barra, lançador, atalhos, compositor). Aqui elas vivem num processo próprio, **`sentinela-desktop.exe`**, que usa só APIs públicas do Windows — **sem injeção**, portanto sem risco de anti-cheat e sem a fragilidade dos mods. Cada peça liga e desliga sozinha; nenhuma é obrigatória.

| Peça | O que faz | Equivalente Linux |
|---|---|---|
| **Barra** | Barra própria em qualquer borda e monitor, com módulos: áreas de trabalho, título da janela, relógio, CPU/GPU/RAM/temperatura, rede, volume, mídia tocando, bateria, bandeja, e "comando" (mostra a saída de um comando seu). Posição, tamanho, fonte, cores, cantos e espaçamento livres. Pode ocultar a barra nativa do Windows | polybar, waybar |
| **Tiling** | Organiza as janelas automaticamente sem sobreposição: layouts (mestre/pilha, colunas, grade, monóculo), espaços entre janelas, foco e movimento pelo teclado, janelas flutuantes por regra | i3, Hyprland, bspwm |
| **Áreas de trabalho** | Áreas numeradas por monitor sobre as áreas virtuais do Windows; mandar janela para a área N; app fixo numa área | workspaces |
| **Lançador** | Caixa de busca por atalho: apps, janelas abertas, arquivos, calculadora, ações do Sentinela | rofi, wofi |
| **Atalhos globais** | Qualquer combinação → ação (abrir app, mover/focar janela, trocar área, layout, tema, comando) | sxhkd |
| **Regras de janela** | Por app/título/classe: flutuar, área, monitor, tamanho, opacidade, sempre no topo, sem barra de título | regras do WM |
| **Bordas e fundo** | Cor e espessura da borda da janela ativa/inativa, cantos retos ou arredondados, fundo Mica/acrílico/transparente em janelas de qualquer app, opacidade por app | picom, bordas do Hyprland |
| **Itens do sistema** | Cursores, fontes do sistema, sons, ícones de pastas e atalhos, cor de destaque | lxappearance |

Decisões de construção (minhas):

- **Barra e lançador desenhados em Rust nativo, sem WebView.** Um app focado em FPS não pode deixar 150 MB de navegador residente; a janela de configuração continua em Tauri porque fecha depois de usar. Meta: `sentinela-desktop` parado abaixo de 1% de CPU e ~30 MB.
- **Configuração em arquivo de texto além da janela** (`%APPDATA%\Sentinela\desktop.toml`, recarregado ao salvar). É o que torna "nível Linux": tudo que a janela faz, o arquivo faz, e dá para versionar. A janela edita o mesmo arquivo.
- **Em jogos:** tela cheia esconde a barra e suspende o tiling naquele monitor. Como não há injeção, o processo não precisa fechar.
- **Módulo "comando" e atalhos que rodam comandos** são escritos pelo próprio usuário; em tema importado de terceiros eles vêm desligados e listados para revisão (§9.5).
- **Saída de emergência:** um atalho fixo (`Win+Ctrl+Shift+Esc`) desliga tudo do §9.3 e devolve a barra nativa, caso uma configuração deixe o desktop inutilizável.
- Bordas e fundo usam atributos do DWM aplicados de fora do processo; o que o DWM não aceitar de fora para um app específico fica sem efeito naquele app (a confirmar caso a caso na implementação).

### 9.4 Wallpaper Engine

O Sentinela **não desenha papel de parede**; ele conversa com o [Wallpaper Engine](https://www.wallpaperengine.io) (Steam, pago, fora do winget — a página Aplicativos abre a loja da Steam). Sem ele instalado, esta seção some e os temas usam o papel de parede estático do Windows.

- **Detecção:** pasta de instalação pela biblioteca da Steam (app 431960), versão e se está rodando.
- **Controle** pela linha de comando oficial do Wallpaper Engine (`-control openWallpaper | pause | play | stop | mute | applyProperties | openPlaylist`; sintaxe exata a confirmar na implementação), atrás de um `trait WallpaperHost` com versão falsa para testes.
- **Galeria:** lista os wallpapers instalados (pastas da Oficina, `project.json` + miniatura) para escolher por monitor de dentro do Sentinela e para guardar em temas. Baixar da Oficina continua sendo feito no Wallpaper Engine.
- **Cores a partir do wallpaper** (como o pywal no Linux): ao trocar o wallpaper, extrai uma paleta da miniatura dele e aplica à cor de destaque do Windows, barra, bordas, lançador e, opcionalmente, ao Windows Terminal. A troca é percebida observando o arquivo de configuração do Wallpaper Engine, então funciona também com playlists e trocas feitas por lá. Pode ser fixada por tema.
- **Jogos:** no Modo Jogo (§6.4) e na parada em jogos, o Sentinela manda `pause` (ou `stop`, que libera também a VRAM — escolha do usuário) e `play` ao sair, independentemente da regra de tela cheia do próprio Wallpaper Engine. Entra no perfil "FPS competitivo".
- **Convivência com o resto:**
  - Nenhum mod ou peça do §9.3 mexe nas janelas da área de trabalho (`Progman`/`WorkerW`) onde o Wallpaper Engine desenha; teste manual obrigatório por mod.
  - Barra, estilo da barra nativa e janelas com fundo translúcido deixam o wallpaper animado aparecer por trás.
  - Com tiling e espaços, nenhuma janela fica "maximizada", então a regra do Wallpaper Engine de pausar com janela maximizada deixa de disparar; o Sentinela oferece "pausar quando a área de trabalho estiver coberta" para compensar.
  - A limpeza nunca toca no conteúdo da Oficina nem na configuração do Wallpaper Engine.

### 9.5 Temas

Um **tema** é um arquivo único (`.sentinela-theme`, um zip com `theme.toml` + imagens/fontes/cursores) que guarda tudo dos §9.1–9.4: ajustes nativos, mods e seus ajustes, barra, tiling, bordas, cores, atalhos e o wallpaper (id da Oficina ou arquivo local + propriedades).

- Aplicar tema = uma prévia do que muda + um passo no Histórico, desfeito de uma vez.
- Exportar/importar para compartilhar. Na importação: itens que rodam comandos vêm desligados; mods incompatíveis com o build atual são pulados com aviso; wallpaper da Oficina ausente mostra o link para assinar.
- O app traz alguns temas prontos (minimalista, translúcido, tiling com barra no topo) como ponto de partida.
- Um tema pode trazer um `.msstyles`; ele só é aplicado se os temas visuais do §9.6 estiverem ligados.

### 9.6 Temas visuais do Windows (`.msstyles`)

Decisão do usuário: incluir. Um `.msstyles` muda o desenho de quase tudo que é clássico no Windows (barras de título, botões, menus, Explorador, painéis), o que bordas e mods não alcançam. O Windows só carrega temas assinados pela Microsoft; para usar os de terceiros é preciso **neutralizar essa verificação**. É a única função do Sentinela que desliga uma checagem do sistema, então é um componente à parte, **desligado por padrão**, com aviso explícito ao ligar.

- **Como:** *hook* em memória da checagem de assinatura de temas nos processos que carregam temas (lista exata a confirmar na implementação; inclui processos de sistema como `winlogon.exe`). **Nenhum arquivo do Windows é alterado no disco** — diferente dos *patchers* que trocam DLLs do sistema —, então `sfc`, atualizações do Windows e a desinstalação continuam limpos: desligar remove tudo.
- **Escopo:** exige administrador. A lista fechada de processos do §9.2 continua valendo para os mods; este componente tem a própria lista, usada **somente** para esse *hook*.
- **Instalar temas:** a janela importa o pacote, **verifica com o próprio antivírus** (§15), copia para `%SystemRoot%\Resources\Themes` e aplica. Guarda o tema anterior para desfazer.
- **Travas:**
  - ponto de restauração antes de ligar;
  - cada tema declara os builds do Windows que suporta; tema sem declaração entra como "experimental";
  - a cada logon o componente compara o build do Windows: se mudou, volta ao tema padrão e fica desligado até o usuário religar (um `.msstyles` feito para outro build é a causa clássica de tela preta no logon);
  - o Modo de Segurança do Windows não carrega o componente; a página mostra o passo a passo de recuperação.
- **Jogos:** este *hook* vive em processos de sistema e **não dá para descarregar durante uma partida**. A parada em jogos do §9.2 não cobre este componente; quem joga títulos com anti-cheat de kernel é avisado disso ao ligar.
- **Código próprio:** o repositório é MIT; as ferramentas conhecidas que fazem isso são GPLv3, então nada é copiado delas.

### 9.7 Terminal (Oh My Posh)

Pedido do usuário (2026-11-17): personalização do terminal com o [Oh My Posh](https://ohmyposh.dev). O Sentinela não reimplementa o prompt; ele instala, configura e mantém o Oh My Posh e o Windows Terminal pela janela. Sem injeção e sem administrador: tudo é arquivo de configuração do próprio usuário.

**Peças**

| Peça | O que o Sentinela faz |
|---|---|
| **Oh My Posh** | Detecta; instala ou atualiza pelo winget (`JanDeDobbeleer.OhMyPosh`, mesmo `PackageManager` do §8) |
| **Fonte Nerd Font** | Instala por usuário uma fonte com ícones (padrão `Meslo`, lista curta para escolher) pelo instalador de fontes do próprio Oh My Posh, sem administrador |
| **Tema do prompt** | Galeria dos temas que vêm com o Oh My Posh, com prévia renderizada pelo próprio `oh-my-posh` (exportação para imagem) e por amostra de texto; tema próprio do usuário por arquivo; editor simples de segmentos (pasta, git, hora, tempo do último comando, status, bateria) que gera o arquivo do tema |
| **Shells** | Liga em PowerShell 7 e Windows PowerShell (perfil `$PROFILE` de cada um). Git Bash e `cmd` (via Clink) ficam como opção "experimental", desligados |
| **Windows Terminal** | Lê e grava o `settings.json` (versão da Loja e avulsa): fonte, tamanho, esquema de cores, transparência, acrílico, imagem de fundo e cursor, por perfil ou para todos |
| **Extras opcionais** | Módulos `Terminal-Icons` (ícones no `ls`) e `PSReadLine` com sugestões por histórico, instalados no escopo do usuário |

**Como mexe nos arquivos (regras)**

- **Bloco marcado:** no perfil do PowerShell o Sentinela só escreve entre `# >>> sentinela >>>` e `# <<< sentinela <<<`. O resto do perfil nunca é tocado. Desligar remove só o bloco.
- **Backup e desfazer:** antes da primeira escrita em cada arquivo (perfil, `settings.json`), uma cópia vai para `%LOCALAPPDATA%\Sentinela\backups\terminal\` com data; cada mudança entra no Histórico com desfazer, como os tweaks.
- **`settings.json`:** é JSON com comentários; o Sentinela altera só as chaves que gerencia, preserva o resto e os comentários, e recusa gravar (com aviso) se o arquivo não puder ser lido sem perda. Windows Terminal aberto não impede: ele recarrega sozinho.
- **Prévia antes de aplicar**, como no resto do app: o diff do bloco e das chaves do `settings.json`.

**Integração com o resto da personalização**

- **Cores do wallpaper (§9.4):** a paleta extraída do Wallpaper Engine pode gerar o esquema de cores do Windows Terminal e as cores do tema do prompt.
- **Temas (§9.5):** um `.sentinela-theme` pode levar o tema do Oh My Posh, a fonte e as configurações do terminal. Na importação o tema do prompt é só um arquivo de configuração; o Sentinela não aceita temas que executem comandos externos (segmentos de comando são desligados e listados para revisão, como os módulos de comando da barra).
- **Desempenho:** o Oh My Posh acrescenta alguns milissegundos a cada prompt. A página mostra o tempo medido do prompt e recomenda desligar segmentos lentos (git em repositórios grandes).

## 10. Telas

```
┌──────────────┬──────────────────────────────────────────────────┐
│ SENTINELA    │  Início                                          │
│              │  ┌───────────┐ ┌───────────┐ ┌───────────┐       │
│ ● Início     │  │ Proteção  │ │ Espaço    │ │ Otimização│       │
│   Jogos      │  │ Banco: 2d │ │ 3,9 GB    │ │ 7 de 11   │       │
│   Limpeza    │  │ [Verificar]│ │ [Limpar]  │ │ [Otimizar]│       │
│   Aplicativos│  └───────────┘ └───────────┘ └───────────┘       │
│   Personalizar│                                                 │
│   Segurança  │                                                  │
│   Disco      │  Atividade recente                               │
│   Histórico  │  • 14:02 Limpeza: 1,4 GB liberados               │
│ ──────────── │  • 13:55 Tweak aplicado: Game DVR desligado      │
│ ⚙ Ajustes    │                                                  │
│ 🔒 Admin: não │                                                  │
└──────────────┴──────────────────────────────────────────────────┘
```

- **Jogos:** perfis no topo, lista de tweaks por grupo com estado, selo de evidência, ícones de reinício/admin, painel lateral de prévia, botões Aplicar e Desfazer.
- **Segurança** (§15): três botões — Rápida, Completa, Personalizada —, progresso com pausar/cancelar, lista de achados por nível com o motivo e as ações, "Não verificados", quarentena com restaurar e apagar, agendamento; atualizar assinaturas (campo para a chave abuse.ch, guardada no Gerenciador de Credenciais do Windows).
- **Personalização:** abas por área (§9.1) com interruptores e prévia ao vivo do estado; seção "Mods" (§9.2) com um cartão por mod: interruptor, ajustes, selo de compatibilidade com o build atual e "Desligar todos".
  Também: "Terminal" (§9.7: estado do Oh My Posh e da fonte, galeria de temas do prompt com prévia, shells ligados, aparência do Windows Terminal), "Temas" (galeria, aplicar, exportar/importar), "Papel de parede" (galeria do Wallpaper Engine por monitor, cores automáticas), e "Desktop" com um editor por peça do §9.3 — a barra tem editor visual de módulos com prévia ao vivo, e cada editor tem o botão "Abrir arquivo de configuração".
- **Histórico:** tudo que foi aplicado, com desfazer.

## 11. Fases

Cada fase termina com `cargo fmt`, `clippy -D warnings`, testes verdes no CI (só `windows-latest`) e algo utilizável.

| Fase | Entrega | Depende de |
|---|---|---|
| 0 | Workspace `core`/`cli`; API do core pronta para a janela (caminhos tolerantes, cancelamento no `finder`, estado do banco, registro de atividade); CLI idêntico; CI passa a rodar só em `windows-latest` | — |
| 1 | Janela Tauri com Início, Limpeza, Disco, Segurança sobre o que já existe; release publica um instalador NSIS com `sentinela-gui` e `sentinela.exe` | 0 |
| 2 | `core::tweaks` (backend, diário, ponto de restauração), catálogo "Comprovado", página Jogos, Histórico; `sentinela tweak list/apply/undo` no CLI | 0 |
| 3 | `core::apps` + página Aplicativos | 1 |
| 4 | Página Personalização com as nativas (§9.1); categorias novas de limpeza; tweaks "Avançado"; atualizações do Windows (§6.5); GPU por jogo; gerenciador de inicialização | 1, 2 |
| S1 | Antivírus na janela (§15): rápida com áreas críticas, inicialização e processos; completa; personalizada; achados com ações; quarentena com apagar; agendamento; atualização automática das assinaturas | 1 |
| S2 | Nível "Risco" por contexto; validação Authenticode; dentro de compactados; cache de verificados; pausa em jogos; clique direito no Explorador | S1 |
| N1 | Rede (§6.6): medição (latência, *jitter*, perda, traceroute, *bufferbloat*), histórico, DNS mais rápido, ajustes de adaptador e Wi‑Fi | 2 |
| N2 | Comparador de rotas com WARP e WireGuard do usuário; rota por jogo; banda em segundo plano no Modo Jogo; QoS em "Avançado" | N1, 3 |
| 5 | Wallpaper Engine (detectar, galeria, aplicar, pausar em jogos, cores a partir do wallpaper) e Temas (formato, aplicar, exportar/importar) sobre o que já existe | 4 |
| 5T | Terminal (§9.7): Oh My Posh, Nerd Font, temas do prompt, perfis do PowerShell, Windows Terminal; entra nos Temas e nas cores do wallpaper | 3, 5 |
| 6 | `sentinela-desktop`: atalhos globais, regras de janela, bordas e fundo, itens do sistema, saída de emergência, `desktop.toml` | 5 |
| 7 | Barra própria e ocultar a barra nativa | 6 |
| 8 | Tiling e áreas de trabalho | 6 |
| 9 | Lançador | 6 |
| 10 | Assinatura de código; atualização automática do app; Modo Jogo sob demanda | 2 |
| 11 | Motor de mods: agente, DLL, protocolo, modo seguro, lista fechada de processos, parada em jogos; mods 1–3 | 4, 10 |
| 12 | Resolução por símbolos; mods 4–5 | 11 |
| 13 | Mods de XAML 6–7 | 12 |
| 14 | Temas visuais `.msstyles` (§9.6) | 10, S1 |

Fases 2 e 3 são independentes entre si e podem andar em paralelo depois da 1; 7, 8 e 9 também, depois da 6. A assinatura de código (fase 10) vem **antes** dos mods: uma DLL não assinada carregada no Explorer é bloqueada ou removida pelo Defender com frequência.

**Isto virou três projetos**, e cada um terá o próprio plano de implementação: (A) fases 0–4, S1–S2 e N1–N2, o app de otimização, rede, limpeza, aplicativos e antivírus; (B) fases 5–9, o ambiente de desktop; (C) fases 10–14, o motor de mods e os temas visuais. S1–S2 só dependem da fase 1 e podem andar em paralelo com 2, 3 e 4. O ambiente de desktop vem antes dos mods porque entrega a maior parte da personalização sem injeção e sem certificado.

## 12. Decisões minhas a confirmar

1. **Tauri 2 + React** como tecnologia da janela (§4).
2. **Nenhum tweak que reduza segurança** (integridade de memória/VBS, mitigações, Defender), mesmo dando FPS (§2). Se você quiser, a alternativa é um grupo "Segurança × desempenho" com aviso forte e fora de qualquer perfil.
3. **Abrir sem admin** e elevar sob demanda (§5.1), em vez de exigir admin sempre como o winutil.
4. **Catálogo de apps curado e embutido** (~60), em vez de importar os ~240 do winutil.
5. **Só winget**, sem Chocolatey.
6. ~~Personalização profunda delegada ao Windhawk~~ — **decidido pelo usuário em 2026-11-16: motor próprio, sem Windhawk** (§9.2). Dentro disso, são minhas: mods só embutidos (sem mods de terceiros), lista fechada de três processos do shell, agente residente só com mod ligado, e a ordem dos sete mods. **Parada dos mods em jogos: decidida pelo usuário, obrigatória**; são minhas a forma de detectar jogo aberto e a opção extra para o Vanguard.
7. Nome e repositório continuam `sentinela`; o CLI segue sendo distribuído.
8. **Assinatura de código antes dos mods** (§11), o que implica comprar um certificado antes da fase 11.
9. **"Nível Linux" traduzido nas oito peças do §9.3**, num processo sem injeção, com barra e lançador nativos (sem WebView) e configuração também em arquivo de texto.
10. **Ambiente de desktop antes do motor de mods** na ordem das fases, e a divisão em três projetos (§11).
11. ~~Temas `.msstyles` não assinados ficam de fora~~ — **decidido pelo usuário em 2026-11-16: incluir** (§9.6). São minhas: componente separado e desligado por padrão, *hook* só em memória sem alterar arquivos do Windows, volta ao tema padrão quando o build muda, e ser a última fase.
13. **Antivírus (§15):** a janela nunca move nada sozinha — só malware confirmado por hash vem pré-marcado para quarentena, e o resto o usuário decide; a "completa" da janela não faz limpeza junto (o `sentinela full` do CLI não muda); novo nível "Risco" abaixo de "Suspeito"; verificação pausa em jogos; continua sem proteção em tempo real, com o Defender ligado.
14. **Terminal com Oh My Posh (§9.7)** — pedido do usuário em 2026-11-17. São minhas: instalar Oh My Posh e fonte pelo winget/instalador do próprio Oh My Posh (sem administrador), PowerShell 7 e Windows PowerShell ligados e Git Bash/`cmd` experimentais, escrita só num bloco marcado do perfil com backup e desfazer, edição só das chaves gerenciadas do `settings.json` do Windows Terminal, fonte padrão Meslo, e a fase 5T no projeto B.
15. **Rede e conexão (§6.6)** — pedido do usuário em 2026-11-17. São minhas: dizer na janela que mudar a rota de verdade exige servidores de terceiros; medir antes e depois de tudo; DNS mais rápido medido na própria máquina; rotas alternativas só por Cloudflare WARP, WireGuard do próprio usuário e comparação com serviços pagos já instalados, sem integração com eles; QoS por jogo em "Avançado"; fases N1–N2 no projeto A.
12. **Wallpaper Engine só pela linha de comando oficial e pelos arquivos dele em leitura**; o Sentinela não baixa da Oficina nem altera a configuração dele por fora.

Decidido pelo usuário em 2026-11-16:

- **Bloqueio de atualizações do Windows** (§6.5). São minhas: os quatro modos em vez de um interruptor, o Sentinela manter as definições do Defender em dia durante o bloqueio, o aviso de dias sem correções e o botão "Atualizar agora".
- **Antivírus do computador inteiro** com verificação rápida, completa e quarentena (§15).
- **Personalização máxima, nível Linux** (§9.3) e **Wallpaper Engine como sistema de papel de parede** (§9.4).
- **Motor de mods completo, parando em jogos** (§9.2). As opções do Explorador de Arquivos, que tinham saído por engano de leitura do risco de anti-cheat, voltaram ao §9.1 por decisão minha: são só registro, sem injeção.
- **Sem versão portátil:** distribuição só por instalador.
- **Só Windows:** o CI deixa de rodar em `ubuntu-latest`. Os backends em memória e a raiz falsa do `clean` continuam, porque servem aos testes, não a outro sistema. Minha: os `#[cfg(windows)]` e os caminhos alternativos para outros sistemas são removidos aos poucos, quando o arquivo for tocado, e não numa limpeza à parte.

## 13. Testes

- `tweaks`: todo o motor contra o backend em memória — aplicar grava o diário antes de escrever; desfazer devolve o valor original, inclusive "valor não existia"; conflito quando o valor foi alterado por fora; catálogo inteiro aplica e desfaz sem resíduo.
- Validação de catálogos em teste (ids, raízes de registro permitidas, serviços fora de uma lista de proibidos: `WinDefend`, `mpssvc`, `EventLog`…; `wuauserv` e `UsoSvc` só podem aparecer no tweak do §6.5, e o teste falha se aparecerem em qualquer outro).
- Atualizações do Windows: cada modo aplica e volta ao estado original no backend em memória; trocar de modo não acumula resíduo do anterior; estado "Parcial" quando um valor foi revertido por fora; a limpeza `windows-update` devolve o serviço ao estado em que o encontrou.
- `apps`: `PackageManager` falso; parsing de saídas reais do winget guardadas como fixtures.
- Categorias novas de limpeza: os mesmos quatro testes obrigatórios do design original.
- `mods`: parada em jogos contra uma lista de processos falsa (jogo abre → descarrega; dois jogos abertos, um fecha → continua parado; último fecha → recarrega; anti-cheat sem jogo listado → descarrega); lista de processos permitidos e recusa de qualquer outro alvo; protocolo do agente contra um shell falso; contador do modo seguro; mod em build não listado nasce desligado; resolução de símbolos contra PDBs de fixture. Os *hooks* em si só são verificáveis em Windows real: roteiro manual por mod em VM, repetido a cada atualização cumulativa do Windows 11 antes de marcar o build como compatível.
- `desktop`: os algoritmos de layout do tiling são funções puras (lista de janelas + área → retângulos) testadas sem Windows; regras de janela, mapa de atalhos e leitura do `desktop.toml` (inclusive arquivo inválido mantém a configuração anterior) idem. Barra e lançador: teste manual em VM, com um e dois monitores e escalas de DPI diferentes.
- Wallpaper Engine: `WallpaperHost` falso; leitura de pastas da Oficina e do arquivo de configuração a partir de fixtures; extração de paleta contra imagens fixas. Teste manual com o Wallpaper Engine real para cada mod e para a barra.
- Temas: exportar → importar devolve a mesma configuração; importação desliga itens com comandos; aplicar e desfazer um tema inteiro não deixa resíduo.
- Rede: medição contra um servidor falso local (TCP) e um resolvedor DNS falso; comparador só recomenda quando a diferença supera a variação medida (amostras geradas); DNS aplicado e desfeito por adaptador no backend em memória; `warp-cli` e WireGuard atrás de traits com versões falsas; rota por jogo liga ao abrir e desliga ao fechar com a lista de processos falsa.
- Terminal: perfil com conteúdo do usuário antes e depois do bloco → ligar, trocar de tema e desligar deixam o resto byte a byte igual; perfil sem bloco, com bloco duplicado ou com marcador quebrado (recusa com aviso); `settings.json` com comentários e chaves desconhecidas preservados depois de alterar fonte e esquema; desfazer restaura o arquivo original. Instalação pelo `PackageManager` falso.
- Antivírus: nenhum malware real, como hoje (EICAR, arquivos gerados, regras de teste). Enumeração de inicialização e processos atrás de um trait com versão falsa; cada sinal de contexto com um arquivo gerado que o dispara e um que não dispara; assinatura válida reduz a pontuação mas nunca anula hash conhecido; bomba de compactação gerada no teste para em cada limite; cache: arquivo inalterado não é relido, alterado é, e regras novas forçam releitura; a janela não move nada sem ação do usuário; travas da quarentena (pasta do Windows, arquivo em uso, compactado).
- Temas visuais: build diferente faz voltar ao tema padrão e desligar; importação passa pelo antivírus. O *hook* em si só é verificável em VM, com roteiro que inclui recuperação pelo Modo de Segurança.
- Frontend: testes de componente para prévia/confirmar; um smoke test de build no CI.
- Teste manual por fase em uma VM Windows 11 com snapshot.

## 14. Riscos

- **Antivírus marcarem o app** (binário não assinado que mexe em registro e serviços): assinatura de código sobe de "futuro" para a fase 10; até lá, documentar.
- **Falsa sensação de segurança:** uma janela de antivírus com "Verificação completa: nada encontrado" parece mais do que é. Sem tempo real e só com assinaturas públicas, o Sentinela é segunda opinião; o texto da janela e o aviso do Defender ligado são parte do requisito, não enfeite.
- **Quarentena de falso positivo quebrar um programa ou o Windows:** só hash conhecido vem pré-marcado, pasta do Windows e arquivos da Microsoft pedem segunda confirmação, e tudo é restaurável.
- **Temas `.msstyles`:** é a função mais arriscada do plano — um tema incompatível pode impedir o logon, o *hook* fica em processos de sistema, não para em jogos, e é exatamente o tipo de comportamento que antivírus (inclusive o Defender) bloqueiam. Mitigado por ficar desligado por padrão, não alterar arquivos do Windows, voltar ao tema padrão quando o build muda e ter recuperação pelo Modo de Segurança; ainda assim pode simplesmente não ser viável em algum build, e nesse caso a fase 14 é cortada sem afetar o resto.
- **Windows sem correções de segurança:** o modo Bloqueado deixa o sistema exposto a falhas já corrigidas pela Microsoft, e isso piora a cada mês — num app que também é antivírus. Mitigado só em parte: definições do Defender mantidas pelo Sentinela, contador de dias na página Início e "Atualizar agora". O modo "Só segurança" dá a estabilidade de build sem esse custo e é o que a janela recomenda.
- **O Windows desfazer o bloqueio:** serviços e tarefas de autorreparo religam o Windows Update; o estado é reconferido e mostrado, sem promessa de bloqueio permanente.
- **Tamanho:** o pedido cresceu de um app de otimização para um ambiente de desktop inteiro. Barra, tiling e lançador são, cada um, do porte de projetos que existem sozinhos. Mitigado pela divisão em três projetos com entregas utilizáveis a cada fase; o projeto A não espera pelos outros.
- **Personalização contra FPS:** barra, tiling, wallpaper animado e mods consomem CPU/GPU, o oposto do objetivo 1. Mitigado por processo nativo leve, pausa do Wallpaper Engine e parada dos mods em jogos; a página Início mostra o consumo dos residentes do Sentinela.
- **Wallpaper Engine mudar a linha de comando ou o formato dos arquivos:** tudo atrás do `WallpaperHost`; se a leitura falhar, a seção fica indisponível com aviso, sem afetar o resto.
- **Tiling brigar com apps que controlam a própria janela** (jogos em janela, instaladores, apps com tamanho fixo): regras padrão que deixam esses apps flutuando, e a saída de emergência.
- **Atualização do Windows reverter ou renomear chaves:** a detecção de estado mostra "Não aplicado" de novo; catálogos versionados por build mínimo.
- **Ganho de FPS menor que a expectativa:** selo de evidência e texto honesto por tweak; sem promessa de número.
- **winget muda formato de saída:** fixtures e parsing tolerante; migrar para a API COM do winget se virar problema.
- **Anti-cheat:** fora dos mods, nenhuma função injeta em processos ou toca em arquivos de jogos. Os mods nunca entram em processos de jogos, mas um anti-cheat de kernel pode estranhar o Explorer modificado; não há como garantir o contrário sem testar jogo a jogo. Mitigação: os mods param sempre que um jogo ou anti-cheat está aberto (§9.2) e a página avisa. Risco residual: a janela de segundos entre o jogo abrir e a DLL sair, e anti-cheats ativos desde o boot.
- **Mods quebrarem a cada atualização do Windows** (risco principal da expansão): as funções internas do shell mudam sem aviso. Mitigado por falha fechada por build e modo seguro, mas o custo é permanente — cada atualização cumulativa pede reteste e, às vezes, reescrita. Sem essa manutenção os mods simplesmente ficam desligados em builds novos.
- **Um antivírus que injeta DLL no Explorer:** é o comportamento que o próprio Sentinela e o Defender tratam como suspeito. Assinatura de código é pré-requisito; a varredura do Sentinela reconhece os próprios binários pelo hash; ainda assim outros antivírus podem alertar.
- **Travar o shell do usuário:** um *hook* errado derruba a barra de tarefas. Modo seguro e `sentinela mods off` são a saída; nenhum mod entra sem roteiro de teste em VM.
- **Download de símbolos:** os mods 4–7 precisam de internet na primeira execução após cada atualização do Windows; sem ela, ficam desligados até conseguir.

## 15. Antivírus

Pedido do usuário: o app faz a função de antivírus do computador inteiro — verificação rápida (áreas críticas e arquivos de maior risco), verificação completa (lê tudo e avalia riscos) e quarentena quando necessário.

O motor já existe (`scan`, `full`, `quarantine`, `restore`; detectores de hash, YARA e heurística de PE). O que muda é o que ele verifica, como avalia e como a janela deixa o usuário agir.

**Limite que não muda:** é um antivírus **sob demanda**. Não há proteção em tempo real, e a detecção depende de assinaturas públicas, então malware novo pode passar. O Microsoft Defender continua ligado, e a janela diz isso; "nada encontrado" nunca é mostrado como garantia.

### 15.1 Modos

| Modo | O que lê | Duração esperada |
|---|---|---|
| **Rápida** | Áreas críticas (abaixo), só tipos de arquivo de risco | minutos |
| **Completa** | Todos os arquivos de todos os discos fixos, inclusive dentro de compactados | horas na primeira vez; bem menos depois (§15.4) |
| **Personalizada** | Pastas ou discos escolhidos, inclusive removíveis; também pelo clique direito no Explorador ("Verificar com o Sentinela") | — |

**Áreas críticas da rápida**, em todos os discos fixos:

1. Executáveis dos **processos em execução**.
2. Tudo que **inicia sozinho**: chaves `Run`/`RunOnce`, pastas de Inicialização, tarefas agendadas e serviços — o arquivo apontado por cada um.
3. Pastas onde malware costuma cair: Downloads, Área de Trabalho, Documentos, `%TEMP%`, `%APPDATA%`, `%LOCALAPPDATA%`, `%ProgramData%`, `Users\Public`, a raiz de cada disco e as extensões dos navegadores.

**Tipos de risco:** executáveis e DLLs (reconhecidos pelo cabeçalho, não pela extensão), scripts (`.ps1`, `.bat`, `.cmd`, `.vbs`, `.js`, `.hta`), instaladores, atalhos `.lnk` (e o alvo), documentos do Office com macro e compactados (primeiro nível).

### 15.2 Avaliação de risco

Hoje há três níveis de achado e só sinais do próprio arquivo. Passa a haver um quarto nível e sinais de **contexto**:

| Nível | Quando |
|---|---|
| **Malicioso** | Hash de malware conhecido, ou regra YARA marcada como maliciosa |
| **Suspeito** | Regra YARA da comunidade, ou heurística de PE acima do limiar |
| **Risco** (novo) | Nada casou, mas o contexto é ruim: executável sem assinatura válida que inicia sozinho a partir de Temp/AppData; extensão dupla (`foto.jpg.exe`) ou que não bate com o conteúdo; script na Inicialização; atalho que chama PowerShell com comando codificado; macro de execução automática |
| Limpo | — |

- Cada achado mostra **por quê** em linguagem simples (qual regra, quais sinais) e o SHA-256.
- A **assinatura Authenticode passa a ser validada de verdade** (hoje só se checa a presença): arquivo com assinatura válida de editor confiável perde pontos na heurística e no contexto, o que reduz falsos positivos. Nunca anula um hash de malware conhecido.

### 15.3 Resultados e quarentena

- **A verificação pela janela nunca move nada sozinha.** No fim, a lista de achados vem com a ação recomendada:
  - *Malicioso por hash*: já marcado para quarentena;
  - *Malicioso por YARA*, *Suspeito* e *Risco*: desmarcados, porque podem ser falso positivo — o usuário escolhe.
- Ações por achado: **Quarentena**, **Ignorar sempre** (lista de exceções por hash, editável em Ajustes), **Abrir pasta**, **Detalhes**, e **Consultar no VirusTotal** (abre o navegador só com o hash; nenhum arquivo é enviado).
- Travas da quarentena:
  - arquivo dentro de `%SystemRoot%` ou com assinatura válida da Microsoft pede uma segunda confirmação que explica o risco de quebrar o Windows;
  - arquivo de processo em execução: oferece encerrar o processo antes; se continuar bloqueado, fica listado como "em uso" (sem remoção no boot nesta versão);
  - achado dentro de compactado: a quarentena é do compactado inteiro, e isso é dito antes.
- **Página Quarentena:** lista, restaurar (regras atuais: não sobrescreve, confere hash) e **apagar definitivamente**, com confirmação de que não tem volta. Os arquivos ficam guardados de forma que não possam ser executados por engano (codificados, se o formato atual ainda não fizer isso).
- **Opção em Ajustes, desligada por padrão:** "Colocar malware confirmado por hash em quarentena automaticamente", usada pelas verificações agendadas.
- A "completa" da janela **não faz limpeza junto**; Limpeza é outra página. O `sentinela full` do CLI continua como é.

### 15.4 Desempenho e cobertura

- **Cache de verificados:** guarda, por arquivo, tamanho, data, identificador e SHA-256. Na próxima completa, arquivo inalterado não é relido: o hash guardado é conferido contra as assinaturas novas, e as regras de conteúdo só rodam de novo em arquivo alterado ou quando as regras mudaram.
- **Segundo plano:** prioridade baixa de CPU e disco, limite de threads, pausar/continuar, cancelar com relatório parcial, e **pausa automática quando um jogo abre** (mesma detecção do §9.2).
- **Compactados:** zip na primeira versão; limites de profundidade, tamanho descompactado e número de entradas contra bombas de compactação. O que passar do limite aparece como "não verificado".
- **Cobertura:** ler o computador inteiro exige administrador. Sem elevação, verifica o que alcança e lista o que ficou de fora com o botão de elevar. Arquivos bloqueados, só na nuvem e links continuam aparecendo em "Não verificados" com o motivo.
- **Agendamento:** verificação rápida semanal pelo Agendador de Tarefas, com notificação do resultado; oferecida no primeiro uso.
- **Assinaturas:** atualização automática ao abrir o app se o banco tiver mais de 24 h; idade do banco visível na página Início.
- Os binários do próprio Sentinela (agente, DLL de mods) são reconhecidos pelo hash da instalação; temas e pacotes importados são verificados antes de aplicar.
