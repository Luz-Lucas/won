# Sentinela — Fase N1: rede (medição, rota, bufferbloat, DNS, adaptador) — Plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Uma página **Rede** que mede antes de mudar qualquer coisa: latência, *jitter* e perda até o roteador, DNS públicos e destinos do usuário; rota salto a salto mostrando onde o atraso começa; teste de *bufferbloat*; histórico para comparar antes e depois; DNS mais rápido medido na própria máquina e aplicado por adaptador (com desfazer); e os ajustes locais de adaptador, mais o estado do Wi‑Fi.

**Architecture:** Um módulo novo `crates/core/src/net/`. A medição fica atrás de `trait Prober` (ICMP e TCP reais; falso nos testes), e as contas (média, *jitter*, perda, onde o atraso começa, diagnóstico do *bufferbloat*) são funções puras. O DNS escolhido e os ajustes de adaptador passam pelo motor de ajustes (prévia, diário, desfazer) como ajustes gerados no grupo novo `Rede`; o DNS usa uma ação nova, `DnsServers`, aplicada pela API `SetInterfaceDnsSettings` (vale na hora, sem reinício).

**Tech Stack:** Rust 2024, windows-sys 0.61 (`IpHelper`, `WiFi`), `ureq` (download do teste de *bufferbloat*), Tauri 2, React + TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-16-sentinela-gui-otimizacao-design.md` — §6.6 (rede), §13 (testes de rede), §11 fase N1.

**Fora deste plano (N2):** comparador de rotas com Cloudflare WARP e WireGuard, rota por jogo, banda em segundo plano no Modo Jogo, QoS por jogo. A varredura de Wi‑Fi desligada "enquanto um jogo estiver aberto" também fica para a N2: depende de saber que há um jogo aberto, que é o mesmo mecanismo da rota por jogo.

## Global Constraints

- Só Windows; CI `windows-latest`; builds locais com `CARGO_TARGET_DIR=E:/st`.
- Toda tarefa termina com `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm test` e `npm run build` verdes.
- **Honestidade (§6.6):** a janela diz que a rota até o servidor do jogo é decidida pelo provedor; nada é vendido como ganho sem medição; o DNS mais rápido afeta a resolução de nomes, não o ping dentro do jogo. Não entram "aceleradores" que mudam dezenas de chaves TCP, MTU forçado, nem desligar IPv6.
- Medir nunca muda nada. Toda mudança (DNS, adaptador) tem prévia, confirmação e desfazer pelo Histórico.
- Testes de medição só contra servidores falsos locais (TCP e DNS em `127.0.0.1`) ou `Prober` falso; nenhum teste depende de internet. Leitura real em `#[ignore]`.
- Textos em pt-BR; `app/src-tauri` sem regra de negócio.

## Review Focus

1. **Perda contada certo:** sondas que não respondem entram na perda e ficam fora da média e do *jitter*; destino que não responde a nada aparece "sem resposta", não "0 ms". → Tarefa 1.
2. **Onde o atraso começa:** salto sem resposta (`*`) não vira "o atraso começa aqui"; o salto apontado é o primeiro cujo aumento se mantém nos saltos seguintes. → Tarefa 2.
3. **DNS desfeito volta ao automático (DHCP)** quando antes era automático, e à lista fixa anterior quando era fixa; IPv4 e IPv6 separados. → Tarefa 3.
4. **Ajuste de adaptador só no que o driver oferece:** palavra-chave que o driver não tem não é criada. → Tarefa 4.

---

### Task 1: Medição (latência, *jitter*, perda) e histórico

**Files:** `crates/core/src/net/{mod,probe,stats,targets,history}.rs`, `crates/core/src/net/windows.rs` (ICMP e gateway).

- `trait Prober { fn probe(&self, target: &Target, timeout: Duration) -> Result<Duration, String>; }`. `IcmpProber` (`IcmpSendEcho2` / `Icmp6SendEcho2`, sem administrador), `TcpProber` (tempo do `connect`, para destinos que não respondem a ping).
- `Target { name, host, kind: Icmp | Tcp { port } }`. Destinos padrão: o roteador (gateway padrão lido de `GetAdaptersAddresses`) e um DNS público (`1.1.1.1`); o usuário acrescenta `host` ou `host:porta` (lista em `%LOCALAPPDATA%\Sentinela\net-targets.json`). *Decisão:* a lista embutida de servidores de jogos por região do §6.6 não entra nesta fase — endereços de servidores de jogos mudam sem aviso e muitos não respondem a ping; inventar uma lista daria números falsos. O usuário acrescenta o servidor que usa (o jogo costuma mostrar o IP).
- `measure(prober, target, count, interval) -> Measurement { sent, received, loss_pct, min, avg, max, jitter }`; *jitter* = média das diferenças absolutas entre respostas consecutivas.
- Histórico: `%LOCALAPPDATA%\Sentinela\net-history.json`, até 200 entradas (`{ at, kind, target, resumo }`), mais recentes primeiro.

- [x] **Step 1: Testes que falham:** `stats_mean_jitter_and_loss`, `no_reply_is_not_zero_ms`, `tcp_prober_against_local_server`, `tcp_prober_refused_is_error`, `user_targets_parse_host_and_port`, `history_caps_and_orders`. `#[ignore]`: ICMP real até o gateway.
- [x] **Step 2–4:** FAIL → implementar → PASS.
- [x] **Step 5: Commit** — `feat(core): network latency, jitter and loss measurement`

### Task 2: Rota (traceroute) e *bufferbloat*

**Files:** `crates/core/src/net/{trace,bufferbloat}.rs`.

- Traceroute próprio por ICMP com TTL crescente (`IcmpSendEcho2` com `IP_OPTION_INFORMATION.Ttl`), até 30 saltos, 3 sondas por salto. `Hop { ttl, addr, rtts }`. Classificação: saltos com endereço privado comum (`10/8`, `172.16/12`, `192.168/16`, link-local, `fc00::/7`) antes do primeiro salto CGNAT ou público = **rede de casa**; o resto = **provedor e além** (sem consulta externa de dono do endereço). *Decisão na implementação:* o CGNAT (`100.64/10`) conta como provedor, não como casa — é comum no Brasil e o endereço é do provedor; na rota real desta máquina, os saltos `10.100.x` aparecem depois do primeiro público e ficam corretamente como provedor.
- `where_delay_starts(hops)`: o primeiro salto cuja latência mínima sobe mais de 20 ms em relação ao anterior que respondeu **e** se mantém nos saltos seguintes (um salto isolado lento é roteador que despriorizou o ICMP, não atraso real).
- *Bufferbloat*: latência ao roteador parado (10 sondas) × durante um download de teste (`https://speed.cloudflare.com/__down?bytes=25000000`, blocos de 25 MB em sequência — pedidos maiores recebem 403 —, por até 15 s, cancelável). Diagnóstico pelo aumento da mediana: até 30 ms "bom"; até 100 ms "moderado"; acima "alto: ative o SQM/QoS no roteador".

- [x] **Step 1: Testes que falham:** `trace_classifies_home_and_isp`, `delay_start_ignores_isolated_slow_hop`, `delay_start_ignores_timeouts`, `bufferbloat_grades`, `bufferbloat_without_baseline_reply_is_unknown`. `#[ignore]`: traceroute real até `1.1.1.1`.
- [x] **Step 2–4:** FAIL → implementar → PASS.
- [x] **Step 5: Commit** — `feat(core): traceroute with delay start and bufferbloat test`

### Task 3: DNS mais rápido

**Files:** `crates/core/src/net/dns.rs`, `crates/core/src/tweaks/{model,backend,state,catalog}.rs`, `crates/core/src/tweaks/windows/dns.rs`.

- Medição: consultas A (IPv4) montadas à mão por UDP, 3 nomes × 3 repetições, mediana por resolvedor. Resolvedores: o atual de cada adaptador e Cloudflare (`1.1.1.1`, `1.0.0.1`), Google (`8.8.8.8`, `8.8.4.4`), Quad9 (`9.9.9.9`, `149.112.112.112`), AdGuard (`94.140.14.14`, `94.140.15.15`), OpenDNS (`208.67.222.222`, `208.67.220.220`), com os pares IPv6 de cada um para quando o adaptador tem IPv6.
- `Action::DnsServers { interface: "{GUID}", ipv6: bool, servers: Vec<String> }` e `Snapshot::DnsServers { servers }` (lista vazia = automático/DHCP). Backend do Windows: `GetInterfaceDnsSettings` / `SetInterfaceDnsSettings` (Windows 10 2004+; vale na hora). Validação: GUID entre chaves, servidores IPv4/IPv6 válidos e da família certa, `needs_admin`.
- Grupo novo `Rede`. Ajuste gerado `dns-{iface}` com IPv4 (e IPv6 quando houver) do resolvedor escolhido.
- *Decisão:* o DNS sobre HTTPS do Windows fica para a N2 — a configuração por servidor ainda precisa ser conferida build a build, e o ganho não é de latência.

- [x] **Step 1: Testes que falham:** `dns_query_and_response_roundtrip`, `benchmark_against_fake_resolvers` (UDP local, um lento e um que não responde), `dns_apply_and_undo_restores_dhcp`, `dns_apply_and_undo_restores_static`, `dns_action_validation`. `#[ignore]`: leitura real do DNS dos adaptadores.
- [x] **Step 2–4:** FAIL → implementar → PASS.
- [x] **Step 5: Commit** — `feat: measure DNS resolvers and apply the fastest per adapter`

### Task 4: Ajustes de adaptador e Wi‑Fi

**Files:** `crates/core/src/net/{adapter,wifi}.rs`.

- Ajuste gerado `nic-{iface}` (situacional, admin, reboot) por adaptador físico, só com as palavras-chave que o driver tem em `HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e972-e325-11ce-bfc1-08002be10318}\NNNN` (ligado pelo `NetCfgInstanceId`): `*EEE` = "0" (Energy-Efficient Ethernet), `*InterruptModeration` = "0", `PnPCapabilities` = 24 (o Windows não desliga o adaptador para economizar energia). Os valores das palavras-chave padronizadas são REG_SZ. "Green Ethernet" é de cada fabricante (sem palavra-chave padrão) e fica de fora.
- Wi‑Fi (`WlanQueryInterface`): se a conexão ativa é Wi‑Fi, SSID, sinal (%) e canal; aviso "Jogar pelo cabo é mais estável que pelo Wi‑Fi".

- [x] **Step 1: Testes que falham:** `nic_tweak_only_existing_keywords`, `nic_tweak_skips_virtual_adapters`, `wifi_channel_from_frequency`. `#[ignore]`: leitura real dos adaptadores e do Wi‑Fi.
- [x] **Step 2–4:** FAIL → implementar → PASS.
- [x] **Step 5: Commit** — `feat(core): network adapter tweaks and Wi-Fi status`

### Task 5: Página Rede

**Files:** `app/src-tauri/src/commands/net.rs`, `app/src/pages/Rede.tsx` (+ teste), `Sidebar.tsx` ("Rede" depois de Personalizar), `App.tsx`, `api.ts`.

- Medições longas como jobs canceláveis (`JobKind::Net`), com progresso.
- Seções: **Medir** (destinos com latência média, *jitter*, perda; "Adicionar destino"; "Rastrear rota" por destino, com a tabela de saltos, a divisão casa/provedor e "o atraso começa aqui"), **Bufferbloat** (botão, resultado e diagnóstico), **DNS** (medir, tabela ordenada, "Usar o mais rápido" com prévia por adaptador, desfazer pelo Histórico; aviso de que afeta nomes, não o ping), **Adaptador** (interruptores dos ajustes gerados, como na Personalizar) e **Wi‑Fi** (estado e aviso), **Histórico** (últimas medições).
- Texto fixo no topo: "A rota até o servidor do jogo é decidida pelo seu provedor e pelas redes no caminho. O Sentinela mede, ajusta o que é do seu computador e mostra o antes e depois."

- [x] **Step 1: Testes que falham** (Vitest): `measure_runs_as_job_and_shows_results`, `trace_marks_delay_start`, `bufferbloat_shows_grade`, `dns_apply_asks_confirmation_with_preview`, `adapter_switch_asks_confirmation`, `wifi_notice`, `sidebar_has_rede`.
- [x] **Step 2–4:** FAIL → implementar → PASS.
- [x] **Step 5: Commit** — `feat(gui): network page`

### Task 6: CLI e roteiro manual

**Files:** `crates/cli/src/net.rs`, README, `docs/testes-manuais-fase-n1.md`.

- `sentinela net test [<destino>...] [--count N] [--json]`, `net trace <destino> [--json]`, `net bufferbloat [--json]`, `net dns [--json]` (só mede), `net history [--json]`.

- [x] **Step 1: Testes que falham** (sem internet e sem mudar nada): `net_test_against_local_tcp_target`, `net_trace_unknown_host_exits_2`, `net_history_runs`, `net_dns_help`.
- [x] **Step 2–4:** FAIL → implementar → PASS.
- [x] **Step 5:** Roteiro manual: medição por cabo e por Wi‑Fi, rota com o atraso no Wi‑Fi × no provedor, *bufferbloat* com e sem SQM, DNS aplicado e desfeito (DHCP e fixo), ajustes de adaptador e reinício.
- [x] **Step 6: Commit** — `feat(cli): net commands; docs for phase N1`
