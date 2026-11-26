# Loja: créditos e licenças

O conteúdo original da Loja (ponteiros, ícones e temas) foi desenhado por código
(`tools/loja/gerar.py`) e é CC0. Os **pacotes de ícones de conjuntos abertos** (seção
"Ícones de terceiros" abaixo) são arte de terceiros, convertida para `.ico`, e mantêm a
licença e a autoria originais.

| Conteúdo | Autor | Licença |
| --- | --- | --- |
| Ponteiros: Noite, Claro, Neon ciano, Magenta pulso, Âmbar grande | Sentinela | CC0 1.0 |
| Ponteiros extras (online): Verde terminal, Grafite, Pôr do sol | Sentinela | CC0 1.0 |
| Ícones: Flat colorido, Linha azul, Neon, Pastel | Sentinela | CC0 1.0 |
| Ícones extras (online): Oceano, Ouro e grafite | Sentinela | CC0 1.0 |
| Temas (combos de ajustes + cor + ponteiro + ícones) | Sentinela | CC0 1.0 |

CC0 1.0: domínio público, sem restrições: <https://creativecommons.org/publicdomain/zero/1.0/>.

## Ícones de terceiros (online)

Todos baixados sob demanda (não vão no executável). Cada card da Loja mostra autor e licença.
Alteração feita por nós: os SVGs foram pintados de preto (ou mantidos coloridos, nos emojis) e
rasterizados em `.ico` de 16 a 256 px, escolhendo 6 glifos por conjunto (pasta, PC, Lixeira
vazia/cheia, usuário, Rede). Gerados por `tools/loja/icones_externos.py` a partir dos pacotes
npm `@iconify-json/*`, que trazem o SVG original e os metadados de licença.

| Conjunto | Autoria | Licença | Atribuição obrigatória? | Pacotes na Loja |
| --- | --- | --- | --- | --- |
| [Fluent UI System Icons](https://github.com/microsoft/fluentui-system-icons) | Microsoft Corporation | MIT | Manter aviso de licença | Fluent preto (cheio, linha) |
| [Phosphor Icons](https://github.com/phosphor-icons/core) | Phosphor Icons | MIT | Manter aviso de licença | Phosphor preto (regular, negrito, cheio) |
| [Tabler Icons](https://github.com/tabler/tabler-icons) | Paweł Kuna | MIT | Manter aviso de licença | Tabler preto (linha, cheio) |
| [Lucide](https://github.com/lucide-icons/lucide) | Lucide Contributors | ISC | Manter aviso de licença | Lucide preto |
| [Bootstrap Icons](https://github.com/twbs/icons) | The Bootstrap Authors | MIT | Manter aviso de licença | Bootstrap preto (linha, cheio) |
| [Material Design Icons](https://github.com/Templarian/MaterialDesign) | Pictogrammers | Apache 2.0 | Manter licença e aviso das alterações | Material preto (cheio, contorno) |
| [Fluent Emoji](https://github.com/microsoft/fluentui-emoji) | Microsoft Corporation | MIT | Manter aviso de licença | Fluent Emoji (colorido) |
| [Twemoji](https://github.com/jdecked/twemoji) | Twitter, Inc. e colaboradores | CC BY 4.0 | **Sim** (este arquivo + card da Loja) | Twemoji (colorido) |
| [Font Awesome Free](https://github.com/FortAwesome/Font-Awesome) | Fonticons, Inc. (Dave Gandy) | CC BY 4.0 (ícones) | **Sim** | Font Awesome preto |
| [game-icons.net](https://game-icons.net) | Lorc, Delapouite e colaboradores | CC BY 3.0 | **Sim** | Game Icons preto |

Textos completos das licenças: MIT, ISC e Apache 2.0 nos repositórios acima (arquivo `LICENSE`);
CC BY 4.0 <https://creativecommons.org/licenses/by/4.0/>; CC BY 3.0
<https://creativecommons.org/licenses/by/3.0/>.

**Não incluídos de propósito:** ícones extraídos de DLLs do Windows (`imageres.dll`,
`shell32.dll`), como os de `HaydenReeve/WindowsIcons` (sem licença, arte da Microsoft); Papirus
(GPL) e OpenMoji (CC BY-SA), por exigirem a mesma licença nos derivados; logotipos de marcas de
aplicativos (marca registrada).

Os pacotes "preto" usam um **quase preto** (#14161A) com um fio claro em volta do desenho, para
aparecerem no fundo claro e no escuro. Os pacotes de terceiros agora trazem 20 papéis: os 6 do
sistema, unidade de disco, 3 tipos de arquivo (texto, zip, scripts) e 10 categorias de aplicativo
(navegador, terminal, código, chat, música, vídeo, jogo, ferramentas, escritório, genérico).
Os desenhos de aplicativo são genéricos (globo, terminal, balão...): **não há logotipo de marca** na
Loja pública.

## Como a Loja funciona

- **Catálogo embutido** (`catalog/loja/loja.zip`): funciona sem internet. Gerado por
  `python3 tools/loja/gerar.py` (precisa de Pillow, cairosvg e dos pacotes npm @iconify-json/* (ICONIFY_DIR)). Contém `index.json`, prévias e os pacotes.
- **Catálogo online** (`catalog/loja/remoto.json` + `catalog/loja/remoto/*.zip` na branch
  `main`): itens extras. O app só baixa pacotes `https`, até 8 MB, e confere o SHA-256 do
  catálogo antes de instalar. Botão "Buscar mais itens online".
- **Baixar** copia o pacote para `%LOCALAPPDATA%\Sentinela\loja\<tipo>\<id>\`. O zip só pode
  conter `manifest.json` e arquivos `.cur`/`.ani`/`.ico` com o cabeçalho certo; qualquer outra
  coisa (nome com `..`, executável, arquivo grande demais) recusa o pacote inteiro.
- **Aplicar ponteiros** grava em `HKCU\Control Panel\Cursors` (uma entrada por papel, mais
  `Scheme Source=1`) e pede ao Windows para recarregar (`SPI_SETCURSORS`).
- **Aplicar ícones** grava `DefaultIcon` em `HKCU\Software\Classes` para: pastas
  (`Directory`), Este Computador, Lixeira (vazia/cheia), pasta do usuário e Rede, e avisa o
  Explorer (`SHCNE_ASSOCCHANGED`). Para ver tudo, reinicie o Explorer (o app oferece).
- **Aplicar ícones** também grava `DefaultIcon` de unidades (`Drive`) e dos tipos de arquivo
  `.txt` (`txtfile`), `.zip` (`CompressedFolder`) e `.bat`/`.cmd` (`batfile`, `cmdfile`), quando o
  pacote traz esses papéis. Pacotes antigos (6 papéis) continuam funcionando.
- **Aplicar nos atalhos** (botão no card do pacote aplicado, com confirmação): troca o ícone dos
  atalhos `.lnk` da Área de Trabalho e do Menu Iniciar **do usuário** por um desenho da categoria do
  programa (decidida pelo nome do atalho e do `.exe` de destino; pastas, páginas da web e apps da
  Microsoft Store não são tocados). Antes de mexer, o WON copia cada `.lnk` para
  `%LOCALAPPDATA%\Sentinela\loja\originais\atalhos\`; **Restaurar atalhos** devolve os arquivos
  originais byte a byte. Atalhos da Área de Trabalho pública e do Menu Iniciar de todos os
  usuários (exigem administrador) ficam de fora. A troca usa o PowerShell (`WScript.Shell`).
- **Reverter** devolve exatamente os valores guardados antes da primeira aplicação do tipo e
  apaga as chaves que a Loja criou. Trocar de pacote não perde o original.
- **Temas** usam o motor de temas existente (prévia, confirmação, um passo de histórico) e,
  em seguida, aplicam o ponteiro e o pacote de ícones do combo.

## Limites conhecidos

- Não foi testado num Windows real (feito em contêiner Linux: lógica, testes e clippy com alvo
  Windows). Roteiro de teste: `docs/testes-manuais-loja.md`.
- O ícone de pasta do Windows 11 usa `Directory\DefaultIcon`; pastas especiais (Documentos,
  Imagens...) e unidades de disco mantêm o ícone próprio.
- O ícone do `.exe` de cada programa e os tipos de arquivo que dependem do programa associado
  (imagens, PDF, vídeos) não são trocados: só `.txt`, `.zip`, `.bat`/`.cmd` e as unidades.
- Ícones de marca (Discord, Spotify, Steam...) não entram na Loja pública (marca registrada);
  se um dia forem feitos, só para uso local na máquina do usuário.
- Os ponteiros de espera animados (`Wait`, `AppStarting`) são `.ani` de 12 quadros gerados à mão.

## Cursores de vsthemes.org (modest-dark, dot-m, windows-11-cursors-concept-v2)
Pacotes `cursor-modest-dark`, `cursor-win11-concept`, `cursor-dot-escuro`, `cursor-dot-claro`: obtidos em vsthemes.org; a licença de uso foi confirmada pelo usuário (Lucas) em 25/11/2026.
