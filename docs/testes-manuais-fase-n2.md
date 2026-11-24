# Roteiro de testes manuais — Fase N2 (rotas alternativas, rota por jogo, QoS, DNS criptografado)

Execute num **PC ou notebook real com Windows 11** (Wi‑Fi e cabo, se possível), com um jogo instalado pela Steam e acesso a um servidor WireGuard seu (um VPS) ou a um `.conf` de teste. Instale o **Cloudflare WARP** e o **WireGuard** pela página Aplicativos. Antes, crie um ponto de restauração. Marque o resultado de cada item: **OK**, **FALHA** (descreva) ou **N/A**.

> Estado desta cópia: roteiro escrito, **ainda não executado**. Nada da N2 que liga túnel, grava QoS, liga DoH ou segura o Wi‑Fi foi testado numa máquina real; a máquina de desenvolvimento não tem WARP nem WireGuard.

| # | Passo | Esperado | Resultado | Observações |
| --- | --- | --- | --- | --- |
| 1 | **QoS por jogo** (Jogos → GPU por jogo, como administrador) | | | |
| 1.1 | Sem administrador, olhar o interruptor "Prioridade de rede" | Travado, com o aviso de administrador | | |
| 1.2 | Como administrador, ligar a prioridade de rede de um jogo | Diálogo cita DSCP 46 e o "Do not use NLA"; ao aplicar, `reg query "HKLM\SOFTWARE\Policies\Microsoft\Windows\QoS" /s` mostra `Sentinela-…` com `Application Name` = o `.exe`, `DSCP Value` = 46, e `HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\QoS` tem `Do not use NLA` = 1 | | |
| 1.3 | Reiniciar, abrir o jogo e capturar com o Wireshark (filtro `ip.dsfield.dscp == 46`) | O tráfego do jogo sai marcado com DSCP 46 | | |
| 1.4 | Ligar em um segundo jogo e desligar no primeiro | A política do primeiro some; a do segundo e o `Do not use NLA` ficam | | |
| 1.5 | Desfazer "QoS local fora de domínio" pelo Histórico | O valor `Do not use NLA` some | | |
| 2 | **DNS sobre HTTPS** (Rede → DNS, como administrador) | | | |
| 2.1 | Medir DNS com Cloudflare ou Google mais rápido | A caixa "Criptografar (DNS sobre HTTPS)" aparece; para AdGuard/OpenDNS, não | | |
| 2.2 | Marcar a caixa e aplicar | Prévia diz "(criptografado, DNS sobre HTTPS)"; Configurações → Rede → Propriedades mostra "Criptografado (DNS sobre HTTPS)" nos servidores | | |
| 2.3 | `Get-DnsClientDohServerAddress` e abrir sites | Os servidores aparecem com o modelo automático; sites abrem normalmente | | |
| 2.4 | Desfazer pelo Histórico | Volta ao DNS anterior (automático ou fixo) e sem criptografia | | |
| 3 | **Rotas alternativas** (Rede → Rotas alternativas) | | | |
| 3.1 | Com o WARP instalado mas nunca aberto | Aparece "Cloudflare WARP" sem interruptor e com "O WARP ainda não foi ativado…" com o botão **Ativar WARP** | | |
| 3.2 | Abrir o WARP uma vez, voltar e ligar o interruptor | Diálogo avisa que o tráfego passa pelo servidor e que anti-cheats podem bloquear VPN; ao confirmar, o WARP conecta (ícone na bandeja) | | |
| 3.3 | Importar um `.conf` com o nome "vps" | O túnel aparece como "WireGuard: vps"; a chave privada não aparece em lugar nenhum; o arquivo está em `%LOCALAPPDATA%\Sentinela\wireguard\vps.conf` | | |
| 3.4 | Sem administrador | O túnel mostra "Ligar e desligar o túnel exige administrador." | | |
| 3.5 | Como administrador, ligar o túnel | `sc query "WireGuardTunnel$vps"` = RUNNING; o app WireGuard mostra o túnel ativo; desligar remove o serviço | | |
| 3.6 | Com ExitLag ou NoPing instalado | Aparece na lista sem interruptor, com o aviso de que se liga pelo próprio programa | | |
| 4 | **Comparador** | | | |
| 4.1 | Escolher um servidor de jogo como destino, marcar WARP e o túnel, "Comparar rotas" | Progresso por rota; tabela com conexão atual, WARP e túnel; "recomendada" só se a diferença passar da variação | | |
| 4.2 | Com o WARP ligado antes de comparar | No fim, o WARP continua ligado e o túnel desligado (cada um como estava) | | |
| 4.3 | Cancelar no meio | As rotas voltam a como estavam; "Comparação cancelada." | | |
| 4.4 | Histórico da página | Linha "Rotas" com o resumo | | |
| 5 | **Rota por jogo** | | | |
| 5.1 | Escolher "Cloudflare WARP" para um jogo, com o WARP desligado, e abrir o jogo | Em até ~5 s o WARP liga; a atividade mostra "Cloudflare WARP ligado para {jogo}" | | |
| 5.2 | Fechar o jogo | O WARP desliga ("desligado (jogo fechado)") | | |
| 5.3 | Ligar o WARP à mão e abrir o jogo | O Sentinela não mexe ("já estava ligado… não vai desligar"); ao fechar o jogo, o WARP continua ligado | | |
| 5.4 | Dois jogos com a mesma rota: abrir os dois, fechar um | A rota só desliga quando o segundo fecha | | |
| 5.5 | Com o jogo aberto e a rota ligada pelo Sentinela, sair do Sentinela pela bandeja | A rota desliga ao sair | | |
| 5.6 | Escolher o túnel WireGuard para um jogo com o Sentinela sem administrador | Ao abrir o jogo, a atividade mostra o erro de administrador; nada mais quebra | | |
| 6 | **Wi‑Fi durante jogos** (no Wi‑Fi) | | | |
| 6.1 | Marcar "Segurar a varredura do Wi‑Fi…" e abrir um jogo | Atividade "Varredura do Wi‑Fi segurada"; `ping -t` ao roteador sem os picos periódicos de varredura (comparar com a opção desmarcada) | | |
| 6.2 | Fechar o jogo, ou matar o Sentinela pelo Gerenciador de Tarefas com o jogo aberto | O Windows volta à varredura normal (a lista de redes do Wi‑Fi se atualiza de novo) | | |
| 7 | **CLI** | | | |
| 7.1 | `sentinela net routes` | Lista `warp`, `wg:vps` e pagos com o estado | | |
| 7.2 | `sentinela net compare servidor:porta` | Tabela, recomendação; rotas voltam como estavam | | |
| 7.3 | `sentinela net route warp on` e `off` | O WARP conecta e desconecta; `net route wg:nada on` encerra com código 2 | | |
