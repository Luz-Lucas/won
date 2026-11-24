#[derive(Debug, PartialEq, Eq)]
pub enum BuildCheck {
    Tested,
    Unknown,
}

/// O build do Windows precisa estar na lista de builds em que o mod foi testado.
pub fn check(build: u32, tested: &[u32]) -> BuildCheck {
    if tested.contains(&build) {
        BuildCheck::Tested
    } else {
        BuildCheck::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lista_vazia_nunca_libera() {
        assert_eq!(check(26100, &[]), BuildCheck::Unknown);
        assert_eq!(check(26100, &[22631, 26100]), BuildCheck::Tested);
    }
}
