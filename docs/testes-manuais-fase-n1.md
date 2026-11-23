# Roteiro de testes manuais — Fase N1 (Rede)

Execute em uma **VM Windows 11 (24H2 ou mais novo) com snapshot** (restaure o snapshot antes de cada rodada) e, para os itens de Wi‑Fi e de adaptador, num **notebook ou PC real** com placa de rede física e Wi‑Fi. Use a conta de um usuário comum com permissão de elevar via UAC. Marque o resultado de cada item: **OK**, **FALHA** (descreva) ou **N/A**.

> Estado desta cópia: roteiro escrito, **ainda não executado**. A coluna Resultado está vazia.

| # | Passo | Esperado | Resultado | Observações |
| --- | --- | --- | --- | --- |
| 1 | **Medir** | | | |
| 1.1 | Abrir a página Rede (na barra lateral, depois de Personalizar) | Texto no topo: "A rota até o servidor do jogo é decidida pelo seu provedor…"; destinos "Roteador (…)" e "DNS da Cloudflare (1.1.1.1)" | | |
| 1.2 | Pelo cabo, clicar em "Medir" | Progresso "Medindo … (1 de N)…" com "Cancelar"; ao fim, média, jitter e perda por destino; o roteador fica perto de 1 ms | | |
| 1.3 | Pelo Wi‑Fi (cabo desligado), reabrir a página e medir de novo | Aviso no topo "Você está no Wi‑Fi … Pelo cabo a conexão é mais estável."; a seção Wi‑Fi mostra a rede, o sinal e o canal com a faixa (2,4 GHz ou 5 GHz ou 6 GHz); o jitter até o roteador sobe em relação ao 1.2 | | |
| 1.4 | Acrescentar `servidor.de.um.jogo:porta` (ex.: um servidor que não responde a ping) e medir | O destino entra na lista com "Remover"; é medido pelo tempo de conexão; um destino que não aceita conexão mostra "sem resposta" e 100% de perda, nunca "0 ms" | | |
| 1.5 | Acrescentar `a.com:99999` | Erro de porta inválida; nada entra na lista | | |
| 1.6 | Clicar em "Cancelar" durante uma medição | A medição para e o botão "Medir" volta | | |
| 2 | **Rota** | | | |
| 2.1 | Pelo cabo, "Rastrear rota" até a Cloudflare | Tabela de saltos; o primeiro é "Sua casa", os seguintes "Provedor e internet" (endereços `100.64.x.x` do CGNAT contam como provedor); `*` onde o salto não responde | | |
| 2.2 | Pelo Wi‑Fi com sinal fraco (longe do roteador), rastrear de novo | Se o atraso já aparece no salto 1, a marca "o atraso começa aqui" fica no roteador e o texto aponta "dentro de casa: confira o roteador, o cabo ou o Wi‑Fi" | | |
| 2.3 | Rastrear até um servidor distante (outro continente) | A marca fica no salto do provedor/backbone onde o tempo sobe e se mantém; texto "no provedor ou depois dele: isso não se ajusta no computador" | | |
| 3 | **Bufferbloat** | | | |
| 3.1 | Clicar em "Testar bufferbloat" sem SQM no roteador | Cerca de 15 s; "Parado", "Ocupado", velocidade do download e o resultado (Bom, Moderado ou Alto) com o diagnóstico; "Alto" fala de SQM/QoS | | |
| 3.2 | Ligar SQM/QoS no roteador (se houver) e testar de novo | O aumento cai; o resultado melhora (idealmente "Bom") | | |
| 4 | **DNS** | | | |
| 4.1 | Clicar em "Medir DNS" | Tabela com o DNS atual ("Atual (…)") e os públicos, do mais rápido ao mais lento; o aviso de que o DNS não muda o ping do jogo | | |
| 4.2 | Se o atual for o mais rápido | Texto "O DNS que você usa já é o mais rápido. Não precisa mudar." e nenhum botão de aplicar | | |
| 4.3 | Se um público for mais rápido, sem administrador | Aviso "Trocar o DNS exige administrador." com "Reiniciar como administrador"; o botão "Usar o mais rápido" fica travado | | |
| 4.4 | Como administrador, com o adaptador em DNS automático (DHCP), clicar em "Usar o mais rápido" | Diálogo com a prévia por adaptador (`automático (DHCP) → …`); ao aplicar, `Get-DnsClientServerAddress` mostra os servidores escolhidos (IPv4 e, se houver IPv6 global, IPv6); vale na hora | | |
| 4.5 | Medir de novo e aplicar outro DNS público | O Histórico fica com uma entrada só por adaptador (`dns-…`), guardando o DNS original | | |
| 4.6 | Desfazer pelo Histórico | O adaptador volta a DNS automático (DHCP) | | |
| 4.7 | Repetir 4.4–4.6 com o adaptador em DNS fixo (ex.: 9.9.9.9 configurado à mão) | O desfazer devolve exatamente o DNS fixo anterior | | |
| 5 | **Adaptador** (máquina real, como administrador) | | | |
| 5.1 | Abrir a seção Adaptador | Um interruptor por adaptador físico presente ("Adaptador {nome}: sem economia de energia"), com os selos Situacional e Exige reinício; adaptadores virtuais (Hyper‑V, VPN, Bluetooth) não aparecem | | |
| 5.2 | Ligar o interruptor e confirmar | A prévia lista `PnPCapabilities → 24` e só as palavras-chave que o driver tem (`*EEE`, `*InterruptModeration`); ao aplicar, pede para reiniciar | | |
| 5.3 | Reiniciar e abrir Gerenciador de Dispositivos → adaptador → Gerenciamento de Energia / Avançado | "Permitir que o computador desligue este dispositivo" desmarcado; Energy-Efficient Ethernet e Moderação de interrupção desligados (se o driver tiver) | | |
| 5.4 | Desfazer pelo Histórico e reiniciar | Os três valores voltam ao que eram (ou somem, se não existiam) | | |
| 6 | **Histórico da página** | | | |
| 6.1 | Depois dos itens acima, olhar a seção Histórico | Uma linha por medição (latência, rota, bufferbloat, DNS), da mais nova para a mais antiga | | |
| 7 | **CLI no terminal** | | | |
| 7.1 | `sentinela net test` | Tabela com roteador, Cloudflare e os destinos acrescentados; código 0 | | |
| 7.2 | `sentinela net test servidor:porta --count 5 --json` | JSON com `received`, `loss_pct`, `avg_ms`, `jitter_ms` | | |
| 7.3 | `sentinela net trace 1.1.1.1` | Saltos com "casa"/"provedor" e, se houver, "<- o atraso começa aqui"; `net trace nao-existe.invalid` encerra com código 2 | | |
| 7.4 | `sentinela net bufferbloat` | Parado, ocupado, download e o resultado | | |
| 7.5 | `sentinela net dns` | Tabela e a recomendação; o DNS do adaptador **não** muda | | |
| 7.6 | `sentinela net history` | As medições dos itens acima, também as feitas na janela | | |
| 7.7 | `sentinela tweak list` | O ajuste `nic-…` aparece; `tweak apply nic-…` como administrador funciona como o item 5.2 | | |
