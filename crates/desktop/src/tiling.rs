//! Tiling (§9.3): layouts como funções puras (área + quantidade → retângulos),
//! testadas sem Windows. Quem lê e move as janelas é `wintile`.

use crate::rules::Rect;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    MasterStack,
    Columns,
    Grid,
    Monocle,
}

impl Layout {
    pub const ALL: [Layout; 4] = [
        Layout::MasterStack,
        Layout::Columns,
        Layout::Grid,
        Layout::Monocle,
    ];

    pub fn next(self) -> Layout {
        let i = Layout::ALL.iter().position(|l| *l == self).unwrap_or(0);
        Layout::ALL[(i + 1) % Layout::ALL.len()]
    }
}

pub const MIN_RATIO: f32 = 0.2;
pub const MAX_RATIO: f32 = 0.8;

#[derive(Debug, Clone, PartialEq)]
pub struct TilingConfig {
    pub layout: Layout,
    /// Espaço entre janelas.
    pub gap: u32,
    /// Espaço entre as janelas e a borda da área.
    pub outer_gap: u32,
    pub master_ratio: f32,
}

/// Divide `len` em `n` partes inteiras, com `gap` entre elas; a sobra vai
/// uma a uma para as primeiras. Devolve (início, tamanho).
fn split(start: i32, len: i32, n: usize, gap: i32) -> Vec<(i32, i32)> {
    let n_i = n as i32;
    let free = (len - gap * (n_i - 1)).max(n_i);
    let (base, extra) = (free / n_i, free % n_i);
    let mut pos = start;
    (0..n_i)
        .map(|i| {
            let size = base + i32::from(i < extra);
            let r = (pos, size);
            pos += size + gap;
            r
        })
        .collect()
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

/// Retângulos para `n` janelas, na ordem da lista (a primeira é a mestra).
pub fn arrange(cfg: &TilingConfig, area: Rect, n: usize) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }
    let og = cfg.outer_gap as i32;
    let gap = cfg.gap as i32;
    let inner = Rect {
        left: area.left + og,
        top: area.top + og,
        right: (area.right - og).max(area.left + og + 1),
        bottom: (area.bottom - og).max(area.top + og + 1),
    };
    let (w, h) = (inner.right - inner.left, inner.bottom - inner.top);
    if n == 1 || cfg.layout == Layout::Monocle {
        return vec![inner; n];
    }
    match cfg.layout {
        Layout::MasterStack => {
            let ratio = cfg.master_ratio.clamp(MIN_RATIO, MAX_RATIO);
            let master_w = (((w - gap) as f32) * ratio) as i32;
            let stack_w = w - gap - master_w;
            let mut out = vec![rect(inner.left, inner.top, master_w, h)];
            for (y, sh) in split(inner.top, h, n - 1, gap) {
                out.push(rect(inner.left + master_w + gap, y, stack_w, sh));
            }
            out
        }
        Layout::Columns => split(inner.left, w, n, gap)
            .into_iter()
            .map(|(x, cw)| rect(x, inner.top, cw, h))
            .collect(),
        Layout::Grid => {
            let cols = (n as f64).sqrt().ceil() as usize;
            let rows = n.div_ceil(cols);
            let mut out = Vec::with_capacity(n);
            for (r, (y, rh)) in split(inner.top, h, rows, gap).into_iter().enumerate() {
                let in_row = if r == rows - 1 {
                    n - cols * (rows - 1)
                } else {
                    cols
                };
                for (x, cw) in split(inner.left, w, in_row, gap) {
                    out.push(rect(x, y, cw, rh));
                }
            }
            out
        }
        Layout::Monocle => unreachable!(),
    }
}

/// Próximo/anterior com volta; `None` se a lista está vazia.
pub fn step(len: usize, idx: Option<usize>, forward: bool) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let i = idx.unwrap_or(if forward { len - 1 } else { 0 });
    Some(if forward {
        (i + 1) % len
    } else {
        (i + len - 1) % len
    })
}

/// Põe o item `idx` como mestre; se já é, troca com o segundo.
pub fn swap_master<T>(list: &mut [T], idx: usize) {
    if list.len() < 2 || idx >= list.len() {
        return;
    }
    list.swap(0, if idx == 0 { 1 } else { idx });
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        left: 0,
        top: 40,
        right: 1920,
        bottom: 1080,
    };

    fn cfg(layout: Layout) -> TilingConfig {
        TilingConfig {
            layout,
            gap: 8,
            outer_gap: 10,
            master_ratio: 0.5,
        }
    }

    fn inside(r: Rect, a: Rect) -> bool {
        r.left >= a.left && r.top >= a.top && r.right <= a.right && r.bottom <= a.bottom
    }

    fn overlap(a: Rect, b: Rect) -> bool {
        a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
    }

    #[test]
    fn every_layout_stays_inside_and_never_overlaps() {
        for layout in [Layout::MasterStack, Layout::Columns, Layout::Grid] {
            for n in 1..=12 {
                let rs = arrange(&cfg(layout), AREA, n);
                assert_eq!(rs.len(), n, "{layout:?} {n}");
                for (i, a) in rs.iter().enumerate() {
                    assert!(inside(*a, AREA), "{layout:?} {n} {a:?}");
                    assert!(a.right > a.left && a.bottom > a.top);
                    for b in &rs[i + 1..] {
                        assert!(!overlap(*a, *b), "{layout:?} {n}: {a:?} {b:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn single_window_fills_the_area_minus_outer_gap() {
        let r = arrange(&cfg(Layout::Grid), AREA, 1);
        assert_eq!(
            r,
            vec![Rect {
                left: 10,
                top: 50,
                right: 1910,
                bottom: 1070
            }]
        );
    }

    #[test]
    fn master_stack_splits_by_ratio() {
        let rs = arrange(&cfg(Layout::MasterStack), AREA, 3);
        // Largura útil 1900, espaço 8: mestra 946, pilha 946.
        assert_eq!(
            rs[0],
            Rect {
                left: 10,
                top: 50,
                right: 956,
                bottom: 1070
            }
        );
        assert_eq!(rs[1].left, 964);
        assert_eq!(rs[1].right, 1910);
        assert_eq!(rs[1].bottom + 8, rs[2].top);
        assert_eq!(rs[2].bottom, 1070);
        let wide = TilingConfig {
            master_ratio: 5.0,
            ..cfg(Layout::MasterStack)
        };
        let rs = arrange(&wide, AREA, 2);
        assert!(rs[1].right - rs[1].left > 300, "razão é limitada a 0,8");
    }

    #[test]
    fn grid_fills_the_last_row() {
        let rs = arrange(&cfg(Layout::Grid), AREA, 3);
        // 2 colunas, 2 linhas; a última linha tem 1 janela que ocupa a largura toda.
        assert_eq!(rs[2].left, 10);
        assert_eq!(rs[2].right, 1910);
        assert_eq!(rs[0].bottom + 8, rs[2].top);
    }

    #[test]
    fn monocle_stacks_all_on_one_rect() {
        let rs = arrange(&cfg(Layout::Monocle), AREA, 4);
        assert!(rs.windows(2).all(|w| w[0] == w[1]));
    }

    #[test]
    fn tiny_areas_do_not_panic() {
        let tiny = Rect {
            left: 0,
            top: 0,
            right: 30,
            bottom: 30,
        };
        for layout in Layout::ALL {
            for n in 0..=9 {
                let _ = arrange(&cfg(layout), tiny, n);
            }
        }
    }

    #[test]
    fn focus_stepping_and_master_swap() {
        assert_eq!(step(0, None, true), None);
        assert_eq!(step(3, Some(2), true), Some(0));
        assert_eq!(step(3, Some(0), false), Some(2));
        assert_eq!(step(3, None, true), Some(0));
        let mut v = vec!['a', 'b', 'c'];
        swap_master(&mut v, 2);
        assert_eq!(v, ['c', 'b', 'a']);
        swap_master(&mut v, 0);
        assert_eq!(v, ['b', 'c', 'a']);
        let mut one = vec!['a'];
        swap_master(&mut one, 0);
        assert_eq!(one, ['a']);
        assert_eq!(Layout::Monocle.next(), Layout::MasterStack);
    }
}
