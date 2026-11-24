# Teste manual: túnel do WARP (WireGuard) e ativação do WARP

Execute no Windows 11 com o **Cloudflare WARP** e o **WireGuard** instalados (página Aplicativos). Estado desta cópia: **ainda não executado**; o código foi conferido só por testes com resposta simulada da Cloudflare e `clippy` para o alvo Windows. Marque **OK**, **FALHA** ou **N/A**.

| # | Passo | Esperado | Resultado |
|---|---|---|---|
| 1 | Rede > Rotas alternativas, com o WARP nunca aberto | "Cloudflare WARP" sem interruptor, com **Ativar WARP** | |
| 2 | Clicar em Ativar WARP e cancelar | Nada muda | |
| 3 | Ativar WARP e confirmar | Em até 15 s o WARP aparece desligado, com interruptor | |
| 4 | **Criar túnel WireGuard do WARP** e confirmar | Aparece "WireGuard: warp"; o botão some; `%LOCALAPPDATA%\Sentinela\wireguard\warp.conf` existe; a chave privada não aparece na janela nem na Atividade | |
| 5 | Como administrador, ligar "WireGuard: warp" | `sc query "WireGuardTunnel$warp"` = RUNNING; abrir 1.1.1.1/help mostra "Using WARP: Yes" (ou IP da Cloudflare) | |
| 6 | Ligar o WARP (app) e o túnel ao mesmo tempo | Anotar se conflitam; o Sentinela recomenda usar um de cada vez | |
| 7 | `sentinela net warp-tunnel --name warp2` sem `--accept-tos` | Recusa e explica os termos | |
| 8 | `sentinela net warp-tunnel --accept-tos --name warp2 --on` como administrador | Cria e liga `wg:warp2` | |
| 9 | Sem internet, criar o túnel | Mensagem "sem conexão com a Cloudflare"; nenhum `.conf` criado | |
| 10 | Comparar rotas (WARP e túnel) até um servidor de jogo | Ambas aparecem na tabela | |
