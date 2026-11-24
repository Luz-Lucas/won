# Roteiro manual: navegação, Estilização, perfis e Rede automática

Só a interface mudou; nada novo roda no sistema. Testado com vitest e `npm run build`; falta olhar no Windows.

1. **Barra lateral:** grupos Desempenho, Conexão, Proteção, Aparência e Sistema; a página abre com uma entrada suave (some com "reduzir animações" ligado no Windows).
2. **Estilização:** o menu mostra um cartão por elemento. Abra "Barra de tarefas": passe o mouse sobre "Esquerda" e confira que a prévia muda o alinhamento sem aplicar nada. Repita em Menu Iniciar, Explorador e Cores e aparência. "Papel de parede", "Terminal" e "Desktop" ficam desativados com o motivo quando o programa não existe.
3. **Otimização → perfis:** marque algumas otimizações, digite um nome e clique em "Salvar seleção como perfil". Feche e abra o app: o perfil continua lá. Selecione outro perfil e volte; use "Atualizar" e "Excluir perfil".
4. **Rede:** não há mais campo de texto. "Analisar minha rede" mede latência, bufferbloat e DNS em sequência. O WARP continua com o botão "Ativar WARP" / "Criar túnel WireGuard do WARP".

Os perfis ficam no armazenamento local do WebView2 (`localStorage`) da conta que abriu o app.
