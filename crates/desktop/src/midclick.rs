//! Clique do meio na barra de tarefas para fechar o programa (em vez de abrir outra instância).
//! A parte que decide qual janela fechar é pura; `winmid.rs` liga isso ao mouse do Windows.

/// Nome do botão da barra sem a parte de contagem: "Firefox - 2 running windows" vira "Firefox".
pub fn app_label(button_name: &str) -> &str {
    let name = button_name.trim();
    let cut = [" - ", " \u{2013} ", " \u{2014} ", ", "]
        .iter()
        .filter_map(|sep| name.rfind(sep))
        .min();
    // Só corta o final se ele parecer a contagem ("2 windows", "em execução"...); o título do
    // app em si pode ter " - " no meio, por isso corta pela última ocorrência.
    match cut {
        Some(i) if i > 0 => name[..i].trim(),
        _ => name,
    }
}

/// Escolhe a janela a fechar. `windows` = (id, título); `foreground` = janela em foco.
/// Fecha só quando não há dúvida: uma única janela combina, ou, entre várias, a que está em foco.
pub fn pick(button_name: &str, windows: &[(isize, String)], foreground: isize) -> Option<isize> {
    let label = app_label(button_name).to_lowercase();
    if label.len() < 2 {
        return None;
    }
    let hits: Vec<isize> = windows
        .iter()
        .filter(|(_, t)| {
            let t = t.to_lowercase();
            t.contains(&label) || label.contains(&t)
        })
        .map(|(id, _)| *id)
        .collect();
    match hits.as_slice() {
        [one] => Some(*one),
        many if many.contains(&foreground) => Some(foreground),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_drops_the_count() {
        assert_eq!(app_label("Firefox - 2 running windows"), "Firefox");
        assert_eq!(
            app_label("Spotify Premium - 1 running window"),
            "Spotify Premium"
        );
        assert_eq!(app_label("Calculadora"), "Calculadora");
        assert_eq!(
            app_label("Notas - Rascunho - 1 janela em execução"),
            "Notas - Rascunho"
        );
    }

    #[test]
    fn picks_only_when_sure() {
        let w = vec![
            (1, "Mozilla Firefox".to_string()),
            (2, "Spotify Premium".to_string()),
        ];
        assert_eq!(pick("Spotify Premium - 1 running window", &w, 1), Some(2));
        assert_eq!(pick("Firefox - 1 running window", &w, 2), Some(1));
        assert_eq!(pick("Outro - 1 running window", &w, 1), None);
        let two = vec![(1, "Firefox A".to_string()), (2, "Firefox B".to_string())];
        assert_eq!(pick("Firefox - 2 running windows", &two, 2), Some(2));
        assert_eq!(pick("Firefox - 2 running windows", &two, 9), None);
    }
}
