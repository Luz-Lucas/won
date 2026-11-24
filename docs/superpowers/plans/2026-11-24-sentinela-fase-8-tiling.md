# Sentinela — Fase 8: tiling — Plano de implementação

**Spec:** §9.3 (peças "Tiling" e "Áreas de trabalho"), §11 fase 8. Depende da fase 6.

**Escopo desta fase:** tiling automático com 4 layouts, foco e troca pelo teclado, janelas flutuantes por regra ou por atalho. **Fica de fora: áreas de trabalho numeradas** (esconder janelas de outras áreas é arriscado — uma queda deixaria janelas sumidas — e as áreas virtuais do Windows não têm API pública estável); entram numa fase própria depois de validar o tiling no PC.

**Decisões:**
- `tiling.rs`: layouts são funções puras (área + quantidade → retângulos): `master-stack`, `columns`, `grid`, `monocle`; testados para não sair da área nem se sobrepor, de 1 a 12 janelas e em áreas minúsculas.
- Seção `[tiling]` (layout, `gap`, `outer_gap`, `master_ratio`) liga o tiling; regra com `float = true` mantém a janela solta. Ações de atalho: `tile-toggle`, `tile-next-layout`, `focus-next`, `focus-prev`, `swap-master`, `toggle-float`, `master-grow`, `master-shrink`.
- Só entram janelas de topo visíveis, não minimizadas, sem dono, com moldura redimensionável (diálogos e instaladores flutuam), não ocultas pelo DWM e não protegidas. Cada monitor usa a própria área útil (já sem a barra da fase 7).
- Compensa as bordas invisíveis das janelas (`DWMWA_EXTENDED_FRAME_BOUNDS`) para os espaços ficarem do tamanho certo.
- Só reaplica quando o conjunto de janelas ou o layout muda: o usuário pode ajustar uma janela à mão até a próxima mudança.
- Em jogo (janela cobrindo o monitor) nada é movido. Desligar, parar ou a saída de emergência devolvem cada janela ao lugar de antes.

**Tarefas:** 1) `tiling.rs` + `[tiling]`, `float` e ações no `config.rs` (testes). 2) `wintile.rs` e integração (clippy msvc). 3) README e `docs/testes-manuais-fase-8.md`.
