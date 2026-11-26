//! Standard Glicko-2 rating calculation for 1v1 Makhos matches.

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

const GLICKO2_SCALE: f64 = 173.7178;
const DEFAULT_TAU: f64 = 0.5;
const CONVERGENCE_TOLERANCE: f64 = 1e-6;

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Glicko2Rating {
    pub rating: f64,
    pub rd: f64,
    pub volatility: f64,
}

impl Default for Glicko2Rating {
    fn default() -> Self {
        Self {
            rating: 1500.0,
            rd: 350.0,
            volatility: 0.06,
        }
    }
}

impl Glicko2Rating {
    pub fn new(rating: f64, rd: f64, volatility: f64) -> Self {
        Self {
            rating,
            rd,
            volatility,
        }
    }

    fn to_mu_phi(self) -> (f64, f64) {
        (
            (self.rating - 1500.0) / GLICKO2_SCALE,
            self.rd.max(1.0) / GLICKO2_SCALE,
        )
    }

    /// Update a player's rating after a single rated game against `opponent`.
    /// `score` is `1.0` for a win, `0.5` for a draw, and `0.0` for a loss.
    pub fn update_single(&self, opponent: &Self, score: f64) -> Self {
        let (mu, phi) = self.to_mu_phi();
        let (mu_j, phi_j) = opponent.to_mu_phi();
        let sigma = self.volatility.max(1e-4);

        let g_j = g(phi_j);
        let e_val = expected_score(mu, mu_j, phi_j);
        let v = 1.0 / (g_j * g_j * e_val * (1.0 - e_val)).max(1e-12);
        let delta = v * g_j * (score - e_val);

        let new_sigma = solve_volatility(phi, sigma, v, delta, DEFAULT_TAU);
        let phi_star = (phi * phi + new_sigma * new_sigma).sqrt();
        let new_phi = 1.0 / ((1.0 / (phi_star * phi_star)) + (1.0 / v)).sqrt();
        let new_mu = mu + (new_phi * new_phi) * g_j * (score - e_val);

        Self {
            rating: 1500.0 + GLICKO2_SCALE * new_mu,
            rd: (GLICKO2_SCALE * new_phi).clamp(30.0, 350.0),
            volatility: new_sigma,
        }
    }
}

/// Simultaneously update both players' ratings after a single game.
/// `score_light` is `1.0` for Light win, `0.5` for draw, `0.0` for Dark win.
pub fn update_pair(
    light: &Glicko2Rating,
    dark: &Glicko2Rating,
    score_light: f64,
) -> (Glicko2Rating, Glicko2Rating) {
    let next_light = light.update_single(dark, score_light);
    let next_dark = dark.update_single(light, 1.0 - score_light);
    (next_light, next_dark)
}

fn g(phi: f64) -> f64 {
    1.0 / (1.0 + (3.0 * phi * phi) / (PI * PI)).sqrt()
}

fn expected_score(mu: f64, mu_j: f64, phi_j: f64) -> f64 {
    1.0 / (1.0 + (-g(phi_j) * (mu - mu_j)).exp())
}

fn solve_volatility(phi: f64, sigma: f64, v: f64, delta: f64, tau: f64) -> f64 {
    let a = (sigma * sigma).ln();
    let tau2 = tau * tau;
    let phi2 = phi * phi;
    let delta2 = delta * delta;

    let f = |x: f64| -> f64 {
        let ex = x.exp();
        let denom = phi2 + v + ex;
        let num = ex * (delta2 - phi2 - v - ex);
        (num / (2.0 * denom * denom)) - ((x - a) / tau2)
    };

    let mut big_a = a;
    let mut big_b = if delta2 > phi2 + v {
        (delta2 - phi2 - v).ln()
    } else {
        let mut k = 1.0;
        while f(a - k * tau) < 0.0 {
            k += 1.0;
            if k > 100.0 {
                break;
            }
        }
        a - k * tau
    };

    let mut f_a = f(big_a);
    let mut f_b = f(big_b);

    for _ in 0..100 {
        if (big_b - big_a).abs() <= CONVERGENCE_TOLERANCE {
            break;
        }
        let big_c = big_a + ((big_a - big_b) * f_a) / (f_b - f_a);
        let f_c = f(big_c);
        if f_c * f_b <= 0.0 {
            big_a = big_b;
            f_a = f_b;
        } else {
            f_a *= 0.5;
        }
        big_b = big_c;
        f_b = f_c;
    }

    (big_a / 2.0).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_players_win_loss_is_symmetric() {
        let a = Glicko2Rating::default();
        let b = Glicko2Rating::default();
        let (a_after, b_after) = update_pair(&a, &b, 1.0);

        assert!(a_after.rating > 1500.0);
        assert!(b_after.rating < 1500.0);
        assert!(((a_after.rating - 1500.0) - (1500.0 - b_after.rating)).abs() < 1e-6);
        assert!(a_after.rd < a.rd);
        assert!(b_after.rd < b.rd);
        assert!(a_after.volatility > 0.0);
    }

    #[test]
    fn draw_between_unequal_players_converges_ratings() {
        let strong = Glicko2Rating::new(1800.0, 80.0, 0.06);
        let weak = Glicko2Rating::new(1400.0, 80.0, 0.06);
        let (strong_after, weak_after) = update_pair(&strong, &weak, 0.5);

        assert!(strong_after.rating < strong.rating);
        assert!(weak_after.rating > weak.rating);
    }

    #[test]
    fn glickman_example_rating_update_is_reasonable() {
        // Player 1500/200/0.06 beats 1400/30/0.06: rating should increase moderately & RD drop
        let p = Glicko2Rating::new(1500.0, 200.0, 0.06);
        let opp = Glicko2Rating::new(1400.0, 30.0, 0.06);
        let next = p.update_single(&opp, 1.0);
        assert!(next.rating > 1540.0 && next.rating < 1600.0);
        assert!(next.rd < 200.0 && next.rd > 150.0);
    }
}
