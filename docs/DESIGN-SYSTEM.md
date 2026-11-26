# Design system do WON

Toda tela nova ou refeita passa por este processo **antes** de ser escrita. Não é opcional.

## Processo obrigatório (planejamento de uso)
1. **Planejar a tarefa do usuário** (skill `superpowers:brainstorming` / planejamento): quem usa, o que quer
   fazer, em quantos passos, o que pode dar errado.
2. **Definir o sistema** (skill `design:design-system`): usar só os componentes e tokens abaixo; componente novo
   só com justificativa e entra neste documento.
3. **Criticar o desenho** (skill `design:design-critique`) e **revisar acessibilidade**
   (`design:accessibility-review`): contraste, teclado, rótulos, foco.
4. **Revisar os textos** (`design:ux-copy`): português claro, sem jargão, erros que dizem o que fazer.
5. Testar em janela pequena (≈ 800 px) e grande; nada pode ficar espremido ou cortado.

## Tokens (`app/src/styles/tokens.css`, `ui.css`)
- Cores: só variáveis (`--bg`, `--surface`, `--surface-2`, `--border`, `--text`, `--text-dim`, `--accent`,
  `--danger`). Nada de cor solta no CSS ou no TSX (exceto prévias de cor escolhida pelo usuário).
- Espaço em escala fixa `--sp-1..6`; altura de controle `--ctl-h` (34 px); raio `--radius`/`--radius-lg`.

## Componentes
- **Campo**: `<label>` com o texto acima do controle; `input`, `select`, `textarea` usam o estilo global.
- **Linha de formulário** `.row`: grade que quebra em colunas (mín. 170 px); checkboxes alinhados à altura dos campos.
- **Grupo** `fieldset` + `legend`; **cartão** `.card`; **botão** `.btn` / `.btn-primary`.
- Foco visível em todo controle (`--focus-ring`). Estados: normal, hover, foco, desabilitado, erro.

## Regras de uso
- Um controle por decisão; opções avançadas ficam recolhidas.
- Toda configuração externa (arquivo, TOML, registro) tem controle na interface.
- Ações destrutivas pedem confirmação e dizem o que vai acontecer.
