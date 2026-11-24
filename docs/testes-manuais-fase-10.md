# Testes manuais da fase 10 (ainda não executado)

1. **Aviso de versão**: em Ajustes, "Versão do Sentinela" mostra a versão instalada e avisa se há uma release mais nova no GitHub. Sem internet, mostra erro sem travar a tela.
2. **Sessão de jogo**: na página Jogos, "Iniciar sessão de jogo" pede confirmação e troca o plano de energia para alto desempenho (`powercfg /getactivescheme` confirma). "Encerrar" volta ao plano anterior. Feche o app no meio e reabra: a sessão continua listada e pode ser encerrada.
3. **Assinatura**: só vale com os segredos `SIGN_PFX_BASE64` (PFX em base64) e `SIGN_PFX_PASSWORD` no repositório. Numa tag, o instalador deve sair assinado (Propriedades > Assinaturas digitais). Sem os segredos, o build segue sem assinatura.
