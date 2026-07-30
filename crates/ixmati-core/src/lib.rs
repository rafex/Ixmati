pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_tdd_red() {
        // TODO: reemplazar por el primer test real.
        // Este test existe para verificar que el harness TDD funciona.
        // Debe fallar hasta que se implemente el codigo correspondiente.
        let result = add(2, 2);
        assert_eq!(result, 5, "TDD bootstrap: este test DEBE fallar. Implementa y corrígelo.");
    }
}
