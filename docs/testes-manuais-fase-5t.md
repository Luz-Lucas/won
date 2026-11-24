# Roteiro de testes manuais — Fase 5T (Terminal com Oh My Posh)

Execute num **PC com Windows 11**, com o Sentinela **instalado**, o **Windows Terminal** e o **PowerShell 7** (e o Windows PowerShell 5). Faça uma cópia manual de `$PROFILE` e do `settings.json` do Windows Terminal antes de começar. Para os passos 6 e 7, o **Wallpaper Engine**. Marque **OK**, **FALHA** (descreva) ou **N/A**.

> Estado desta cópia: roteiro escrito, **ainda não executado**. Fora do Windows foram conferidos só a lógica pura (edição do bloco do perfil e do `settings.json` com JSONC, backup e desfazer, revisão de temas, cores com contraste, integração com os temas do Sentinela) e a compilação para Windows (`clippy` no alvo `x86_64-pc-windows-msvc`). **Nada que fala com o winget, com o Oh My Posh ou com o Windows Terminal reais foi executado.**

| # | Passo | Esperado | Resultado | Observações |
| --- | --- | --- | --- | --- |
| 1 | **Oh My Posh** | | | |
| 1.1 | Sem o Oh My Posh: Personalizar → Terminal → **Instalar o Oh My Posh** | Pede confirmação; instala pelo winget sem administrador; a tela passa a mostrar a versão | | |
| 1.2 | Com atualização disponível | Aparece **Atualizar** | | |
| 2 | **Fonte** | | | |
| 2.1 | **Instalar Meslo** e confirmar | Fonte "MesloLGM Nerd Font" aparece nas fontes do Windows | | |
| 2.2 | Outra fonte da lista | Idem; nada fora da lista é oferecido | | |
| 3 | **Tema do prompt** | | | |
| 3.1 | Escolher um tema | A amostra aparece (sem códigos de cor); nenhum arquivo muda | | |
| 3.2 | **Medir o tempo do prompt** | Mostra a mediana em ms; avisa se estiver lento (git em repositório grande) | | |
| 3.3 | **Ver o que muda** | Diálogo com o diff do tema, do perfil e (se marcado) do `settings.json`; **nada gravado ainda** | | |
| 3.4 | Cancelar | Nada muda | | |
| 3.5 | Repetir e **Aplicar**; abrir um PowerShell novo | Prompt com o tema; ícones corretos se a fonte do terminal for a Nerd Font | | |
| 3.6 | Abrir o `$PROFILE` | Só o bloco entre `# >>> sentinela >>>` e `# <<< sentinela <<<` foi adicionado; o resto idêntico (quebras de linha e BOM preservados) | | |
| 3.7 | Perfil com os marcadores quebrados à mão (só o de início) | O Sentinela **recusa** e mostra o aviso; nada é gravado | | |
| 3.8 | Marcar os extras e aplicar | `Terminal-Icons` e as sugestões pelo histórico passam a valer no perfil | | |
| 4 | **Windows Terminal** | | | |
| 4.1 | Marcar a fonte e aplicar | O `settings.json` ganha só `profiles.defaults.font.face`; comentários e vírgulas finais intactos | | |
| 4.2 | `settings.json` com comentários, vírgulas finais e chaves fora de ordem | Edição correta; se algo for incomum, o Sentinela recusa em vez de arriscar | | |
| 4.3 | Instalação Store, Preview e sem pacote (se houver) | Cada `settings.json` existente é atualizado | | |
| 4.4 | Com o Windows Terminal aberto | A mudança aparece sem reiniciar (ele recarrega o arquivo) | | |
| 5 | **Desfazer** | | | |
| 5.1 | **Desfazer a última alteração** | O arquivo volta byte a byte ao de antes; o histórico encolhe | | |
| 5.2 | Editar o `settings.json` à mão e desfazer | Pergunta "o arquivo mudou depois do Sentinela"; só desfaz com confirmação | | |
| 5.3 | **Tirar o prompt do perfil** | Só o bloco sai; o resto do perfil fica igual | | |
| 5.4 | Conferir a cópia permanente `.original.bak` | Existe uma por arquivo mexido, com o conteúdo da primeira vez | | |
| 6 | **Cores do wallpaper** | | | |
| 6.1 | Marcar "esquema a partir do papel de parede" e aplicar | Esquema "Sentinela" aplicado como padrão; texto legível (contraste) | | |
| 6.2 | Trocar o wallpaper e repetir | O esquema muda; um só esquema "Sentinela" no arquivo (não duplica) | | |
| 7 | **Temas do Sentinela** | | | |
| 7.1 | Temas → **Ver mudanças** em "Translúcido" | A prévia inclui a fonte e as cores do terminal | | |
| 7.2 | Aplicar e depois **Desfazer tema** | O terminal volta junto com o resto, na ordem | | |
| 7.3 | Mexer no terminal depois do tema e desfazer o tema | Aviso de que o terminal foi alterado depois; o resto do tema é desfeito | | |
| 7.4 | Exportar um tema com prompt e importar de novo | O prompt vai num arquivo `prompt.omp.json`; segmentos que rodam comandos vêm desligados | | |
| 8 | **Segurança** | | | |
| 8.1 | Tema do Oh My Posh com segmento `command` | Aparece em "Desligados por segurança" e não entra no arquivo gravado | | |
| 8.2 | Rodar o Sentinela fora do Windows Terminal/PowerShell (sem eles instalados) | A seção Terminal não aparece | | |
