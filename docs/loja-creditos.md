# Loja: créditos e licenças

Todo o conteúdo que acompanha o Sentinela na Loja é **original do projeto** e
foi desenhado por código (`tools/loja/gerar.py`), sem copiar arte de terceiros.
Por isso não há créditos externos a repartir.

| Conteúdo | Autor | Licença |
| --- | --- | --- |
| Ponteiros: Noite, Claro, Neon ciano, Magenta pulso, Âmbar grande | Sentinela | CC0 1.0 |
| Ponteiros extras (online): Verde terminal, Grafite, Pôr do sol | Sentinela | CC0 1.0 |
| Ícones: Flat colorido, Linha azul, Neon, Pastel | Sentinela | CC0 1.0 |
| Ícones extras (online): Oceano, Ouro e grafite | Sentinela | CC0 1.0 |
| Temas (combos de ajustes + cor + ponteiro + ícones) | Sentinela | CC0 1.0 |

CC0 1.0: domínio público, sem restrições: <https://creativecommons.org/publicdomain/zero/1.0/>.

## Como a Loja funciona

- **Catálogo embutido** (`catalog/loja/loja.zip`): funciona sem internet. Gerado por
  `python3 tools/loja/gerar.py` (precisa de Pillow). Contém `index.json`, prévias e os pacotes.
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
- **Reverter** devolve exatamente os valores guardados antes da primeira aplicação do tipo e
  apaga as chaves que a Loja criou. Trocar de pacote não perde o original.
- **Temas** usam o motor de temas existente (prévia, confirmação, um passo de histórico) e,
  em seguida, aplicam o ponteiro e o pacote de ícones do combo.

## Limites conhecidos

- Não foi testado num Windows real (feito em contêiner Linux: lógica, testes e clippy com alvo
  Windows). Roteiro de teste: `docs/testes-manuais-loja.md`.
- O ícone de pasta do Windows 11 usa `Directory\DefaultIcon`; pastas especiais (Documentos,
  Imagens...) e unidades de disco mantêm o ícone próprio.
- Os ponteiros de espera animados (`Wait`, `AppStarting`) são `.ani` de 12 quadros gerados à mão.
