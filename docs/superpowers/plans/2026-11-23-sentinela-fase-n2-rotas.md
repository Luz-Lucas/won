# Sentinela — Fase N2: rotas alternativas, rota por jogo, QoS e DNS criptografado — Plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Testar rotas que o usuário pode de fato usar e só recomendar quando a medição mostra ganho: comparador lado a lado entre a conexão normal, o Cloudflare WARP e o túnel WireGuard do próprio usuário (e a conexão de serviços pagos já instalados); rota escolhida por jogo, ligada ao abrir o jogo e desligada ao fechar; Wi‑Fi sem varredura em segundo plano enquanto um jogo está aberto; QoS por jogo em "Avançado"; e o DNS sobre HTTPS do próprio Windows na troca de DNS.

**Architecture:** `core::games::running` lê os processos e diz que jogos detectados estão abertos (função pura de diferença entre duas leituras). `core::net::routes` fala com as alternativas por trás de `trait RouteBackend` (WARP por `warp-cli`, WireGuard por `wireguard.exe` e pelo serviço `WireGuardTunnel$nome`; falso nos testes) e o comparador alterna as rotas em rodadas intercaladas, sempre voltando cada uma ao estado em que a encontrou. Uma vigia na janela (thread) junta as duas coisas: ao abrir um jogo, liga a rota escolhida e segura a varredura do Wi‑Fi; ao fechar, desfaz. QoS e DoH passam pelo motor de ajustes (prévia, diário, desfazer).

**Tech Stack:** Rust 2024, windows-sys 0.61 (`ToolHelp`, `WiFi`, `IpHelper`, `Services`), Tauri 2, React + TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-16-sentinela-gui-otimizacao-design.md` — §6.6 (rede, parte 3 "Testar rotas alternativas" e a tabela de ajustes locais), §11 fase N2.

## Decisões

- **Banda em segundo plano fica para a fase 10**, junto com o Modo Jogo sob demanda (§6.4) de que ela depende. Medir o uso de rede por processo no Windows sem administrador exige ETW ou estatísticas por conexão (que pedem administrador para ligar); não vale fazer pela metade aqui.
- **DNS sobre HTTPS pela API oficial**: `SetInterfaceDnsSettings` com `DNS_INTERFACE_SETTINGS3`, `DNS_SETTING_DOH` e o modelo automático do Windows (`DNS_DOH_SERVER_SETTINGS_ENABLE_AUTO`), só para servidores que o Windows já conhece (`DohWellKnownServers`). A Microsoft diz que as chaves de registro `DohInterfaceSettings` dos testes iniciais não valem mais.
- **Wi‑Fi**: `WlanSetInterface` com `wlan_intf_opcode_background_scan_enabled = FALSE` e `wlan_intf_opcode_media_streaming_mode = TRUE`, com o *handle* da WLAN aberto enquanto o jogo roda. O Windows desfaz os dois quando o *handle* fecha ou a conexão cai, então nada fica preso se o app fechar ou travar. Fica fora do diário (não é persistente) e é opcional (desligado por padrão).
- **QoS**: política de QoS local (`HKLM\SOFTWARE\Policies\Microsoft\Windows\QoS\Sentinela-{id}`, valores REG_SZ `Version`, `Application Name`, `Protocol`, portas, IPs, `DSCP Value` = 46, `Throttle Rate` = -1) e, num ajuste separado e compartilhado, `HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\QoS\Do not use NLA` = "1" (sem ele a política só vale em rede de domínio). Separado para que desfazer o QoS de um jogo não quebre o dos outros. Controverso: só ajuda se o roteador respeitar a marcação.
- **WARP**: `warp-cli status|connect|disconnect` (e `registration show`); o Sentinela nunca registra conta nem muda o modo. Se o WARP não está registrado, mostra "Abra o WARP uma vez para ativar".
- **WireGuard**: o `.conf` importado vai para `%LOCALAPPDATA%\Sentinela\wireguard\{nome}.conf` (nome validado: letras, números, `_=+.-`, até 32); ligar e desligar usa `wireguard.exe /installtunnelservice` e `/uninstalltunnelservice` (exige administrador); o estado vem do serviço `WireGuardTunnel$nome`. A chave privada nunca aparece na janela, no CLI nem em log.
- **Serviços pagos** (ExitLag, NoPing): só detectados (instalado e, se o processo está aberto, "ligado agora"). Entram no comparador como a conexão atual quando o usuário os liga; o Sentinela não liga nem desliga.
- **Recomendação** do comparador: a alternativa só é recomendada quando a mediana dela é menor que a da conexão normal por pelo menos 5 ms **e** pela soma dos dois *jitters*, sem perda maior; rodadas intercaladas (normal, A, B, normal, A, B…) para que a variação da hora não favoreça ninguém.
- **Rota por jogo** com WireGuard exige o app como administrador (o túnel é um serviço); com WARP, não.

## Global Constraints

- Só Windows; CI `windows-latest`; builds locais com `CARGO_TARGET_DIR=E:/st`.
- Toda tarefa termina com `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm test` e `npm run build` verdes.
- Honestidade (§6.6): antes de ligar qualquer túnel, a janela avisa que o tráfego passa pelo servidor daquele serviço e que alguns jogos e anti-cheats bloqueiam VPN. Nada é recomendado sem medição.
- Toda alternativa volta ao estado em que estava antes do comparador ou da sessão de jogo (inclusive com o comparador cancelado).
- Testes sem internet e sem mexer em túneis reais (`RouteBackend` falso); leitura real em `#[ignore]`.

## Review Focus

1. **Estado devolvido:** comparador cancelado ou com erro deixa cada rota como estava. → Tarefa 3.
2. **Sessão de jogo:** abrir dois jogos e fechar um não desliga a rota do outro; fechar o app com um jogo aberto desliga o que o Sentinela ligou. → Tarefa 4.
3. **QoS desfeito** não apaga o "Do not use NLA" usado por outro jogo. → Tarefa 1.
4. **DoH desfeito** volta ao DNS e à criptografia que havia antes. → Tarefa 2.

---

### Task 1: QoS por jogo

**Files:** `crates/core/src/games/qos.rs`, `tweaks/catalog.rs` (raízes permitidas), comandos e seção "Por jogo" da página Jogos.

- Ajuste gerado `qos-{hash do exe}` (grupo Avançado, Controverso, administrador, efeito imediato) e o ajuste compartilhado `qos-allow-local`; ligar o QoS de um jogo aplica os dois.
- Testes: `qos_tweak_values`, `qos_undo_keeps_shared_nla`, `qos_validation`; Vitest `qos_switch_applies_with_allow_local`.
- Commit `feat: QoS per game (DSCP 46) in Avançado`.

### Task 2: DNS sobre HTTPS

**Files:** `tweaks/model.rs` (`DnsServers { doh }`), `tweaks/windows/dns.rs`, `net/dns.rs`, página Rede.

- `Action::DnsServers` ganha `doh: bool` (`#[serde(default)]`); o retrato guarda servidores e se havia DoH. Leitura e escrita pela versão 3 das configurações de DNS do adaptador.
- Na troca de DNS, a caixa "Criptografar (DNS sobre HTTPS)" aparece só se o resolvedor está em `DohWellKnownServers`.
- Testes: `doh_round_trip_in_memory`, `doh_offered_only_for_known_servers`; Vitest `dns_apply_with_doh`.
- Commit `feat: DNS over HTTPS when switching DNS`.

### Task 3: Rotas alternativas e comparador

**Files:** `crates/core/src/net/routes.rs` (+ `routes/windows.rs`).

- `RouteBackend` com `status`, `set`. WARP, WireGuard (importar `.conf`, listar, remover) e detecção de serviços pagos.
- Comparador intercalado e recomendação pela regra das Decisões; histórico `Kind::Route`.
- Testes: `compare_restores_states_on_cancel`, `recommend_only_beyond_variation`, `wireguard_name_validation`, `conf_import_hides_private_key`, `warp_status_parsing`.
- Commit `feat(core): route alternatives and comparator`.

### Task 4: Jogo aberto, rota por jogo e Wi‑Fi

**Files:** `crates/core/src/games/running.rs`, `crates/core/src/net/session_game.rs`, vigia na janela.

- `running(exes)` pela lista de processos; `diff(antes, agora)`. Rota escolhida por jogo em `net-game-routes.json`. Contagem de jogos abertos por rota (dois jogos com a mesma rota: desliga só quando o último fecha) e só desliga o que o próprio Sentinela ligou.
- Wi‑Fi: opção "Segurar a varredura do Wi‑Fi durante jogos" nas configurações.
- Testes: `diff_started_and_stopped`, `two_games_share_a_route`, `route_left_alone_if_already_on`, `stop_all_on_exit`.
- Commit `feat: per-game route and Wi-Fi scan hold while a game runs`.

### Task 5: Janela

- Rede: seção "Rotas" (estado de cada alternativa, importar `.conf`, comparar até um destino com a tabela lado a lado e a recomendação, aviso de túnel) e "Rota por jogo"; opção do Wi‑Fi.
- Testes Vitest: `routes_list_and_compare`, `tunnel_warning_before_enabling`, `per_game_route_select`, `wifi_hold_setting`.
- Commit `feat(gui): routes, per-game route and Wi-Fi hold`.

### Task 6: CLI e roteiro manual

- `sentinela net routes [--json]`, `net compare <destino> [--rounds N] [--json]`, `net route (warp|wireguard <nome>) (on|off)`.
- Testes CLI sem internet: `net_routes_runs`, `net_compare_help`, `net_route_unknown_tunnel_exits_2`.
- README e `docs/testes-manuais-fase-n2.md`.
- Commit `feat(cli): route commands; docs for phase N2`.
