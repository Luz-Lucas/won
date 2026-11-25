# Roteiro de teste: Loja (Estilização → Loja)

1. Abra Estilização → Loja. Devem aparecer 15 itens com prévia (5 ponteiros, 4 ícones, 6 temas).
2. **Ponteiro:** em "Noite", clique **Baixar**, depois **Aplicar**. O mouse muda na hora
   (seta, mão sobre links, I-beam em campos de texto, círculo girando ao abrir apps).
   Confira também Configurações → Bluetooth e dispositivos → Mouse → Ponteiro do mouse.
3. Aplique outro ponteiro por cima e depois **Reverter**: tem de voltar ao ponteiro original
   do Windows (não ao primeiro da Loja).
4. **Ícones:** em "Flat colorido", **Baixar e aplicar**; aceite reiniciar o Explorer. Pasta, Este
   Computador, Lixeira (vazia/cheia), sua pasta e Rede mudam. **Reverter** devolve os do Windows.
5. **Tema:** "Noite neon" → **Aplicar tema** → confira a lista → confirmar. Barra/cores mudam, o
   ponteiro e os ícones também. **Desfazer** reverte tudo.
6. **Online:** "Buscar mais itens online" (precisa do `remoto.json` na `main`). Devem entrar
   3 ponteiros e 2 pacotes de ícones com selo "Online". Sem internet, a lista embutida segue
   funcionando e aparece um aviso.
7. Em caso de problema: Loja → Reverter, ou apague `%LOCALAPPDATA%\Sentinela\loja\estado.json`
   depois de restaurar `HKCU\Control Panel\Cursors` pelas opções de Mouse do Windows.

## Ícones de aplicativos, unidades e tipos de arquivo (pacotes "preto" de terceiros)

1. Loja > Pacotes de ícones > **Fluent preto (cheio)** > "Baixar e aplicar". Reinicie o Explorer.
   Esperado: pastas, Este Computador, Lixeira, unidades de disco, `.txt`, `.zip` e `.bat` com o
   desenho novo (preto com fio claro; confira no tema claro e no escuro).
2. No card (aplicado) > **Aplicar nos atalhos** > confirmar. Esperado: os atalhos da Área de
   Trabalho e do Menu Iniciar (navegador, terminal, jogos...) mostram o desenho da categoria;
   pastas e apps da Microsoft Store continuam iguais; o aviso diz quantos atalhos mudaram.
3. Abra um atalho trocado: ele ainda abre o programa certo (destino e argumentos intactos).
4. **Restaurar atalhos**. Esperado: ícones originais de volta; a pasta
   `%LOCALAPPDATA%\Sentinela\loja\originais\atalhos\` fica vazia.
5. **Reverter** no card: unidades, `.txt`, `.zip`, `.bat` voltam ao que eram.
6. Opcional: aplique um pacote antigo (6 papéis, ex.: "Flat colorido") e depois um de 20 papéis;
   reverter tem que devolver também unidades e tipos de arquivo.

