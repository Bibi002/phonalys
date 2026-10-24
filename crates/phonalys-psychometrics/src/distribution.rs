//! Loi normale centrée réduite.

/// Fonction de répartition Φ(z).
pub fn normal_cdf(z: f64) -> f64 {
    0.5 * libm::erfc(-z / std::f64::consts::SQRT_2)
}

/// Quantile Φ⁻¹(p), algorithme d'Acklam (erreur relative < 1.2e-9).
/// Renvoie `None` hors de l'intervalle ouvert ]0, 1[.
pub fn normal_quantile(p: f64) -> Option<f64> {
    if !(p > 0.0 && p < 1.0) {
        return None;
    }
    const A: [f64; 6] = [
        -3.969683028665376e1,
        2.209460984245205e2,
        -2.759285104469687e2,
        1.38357751867269e2,
        -3.066479806614716e1,
        2.506628277459239e0,
    ];
    const B: [f64; 5] = [
        -5.447609879822406e1,
        1.615858368580409e2,
        -1.556989798598866e2,
        6.680131188771972e1,
        -1.328068155288572e1,
    ];
    const C: [f64; 6] = [
        -7.784894002430293e-3,
        -3.223964580411365e-1,
        -2.400758277161838e0,
        -2.549732539343734e0,
        4.374664141464968e0,
        2.938163982698783e0,
    ];
    const D: [f64; 4] = [
        7.784695709041462e-3,
        3.224671290700398e-1,
        2.445134137142996e0,
        3.754408661907416e0,
    ];
    const P_LOW: f64 = 0.02425;

    let tail = |q: f64| {
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };
    let z = if p < P_LOW {
        tail((-2.0 * p.ln()).sqrt())
    } else if p <= 1.0 - P_LOW {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        -tail((-2.0 * (1.0 - p).ln()).sqrt())
    };
    Some(z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn reference_values() {
        assert!((normal_cdf(0.0) - 0.5).abs() < 1e-12);
        assert!((normal_cdf(-1.0) - 0.158_655_253_9).abs() < 1e-9);
        assert!((normal_cdf(-2.0) - 0.022_750_131_9).abs() < 1e-9);
        assert!((normal_cdf(1.96) - 0.975_002_104_9).abs() < 1e-9);
        assert!((normal_quantile(0.975).unwrap() - 1.959_963_985).abs() < 1e-8);
        assert!((normal_quantile(0.03).unwrap() + 1.880_793_608).abs() < 1e-8);
    }

    #[test]
    fn quantile_rejects_out_of_range() {
        for p in [0.0, 1.0, -0.1, 1.1, f64::NAN] {
            assert!(normal_quantile(p).is_none());
        }
    }

    proptest! {
        #[test]
        fn quantile_inverts_cdf(z in -6.0f64..6.0) {
            let back = normal_quantile(normal_cdf(z)).unwrap();
            prop_assert!((back - z).abs() < 1e-6, "z={z} back={back}");
        }

        #[test]
        fn cdf_is_monotonic(a in -8.0f64..8.0, b in -8.0f64..8.0) {
            if a < b { prop_assert!(normal_cdf(a) <= normal_cdf(b)); }
        }
    }
}
