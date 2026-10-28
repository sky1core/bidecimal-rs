use crate::kernel::{Wide, fit};
use crate::layout::{InRange, Layout, Unpacked, W32, W64, W128, Width};
use crate::refbid::Finite;
use crate::test_support::*;
use crate::{Error, Rounding};

type FitAt = fn(Wide, i64, Option<Rounding>) -> Result<Unpacked, Error>;

fn fit_at<W: Width>(w: Wide, pref: i64, dir: Option<Rounding>) -> Result<Unpacked, Error> {
    fit::<W>(w, pref, dir).map(InRange::unpacked)
}

const WIDTHS: [(&Layout, FitAt); 3] = [
    (&W32::LAYOUT, fit_at::<W32>),
    (&W64::LAYOUT, fit_at::<W64>),
    (&W128::LAYOUT, fit_at::<W128>),
];

#[test]
fn fit_full_integer_domain_boundaries_and_sticky_rational_oracle() {
    let powers = Powers::new(13000);
    for (format, (_, fit)) in FORMATS.into_iter().zip(WIDTHS) {
        let (p, lo, hi) = bounds(format);
        let mut coeffs = coefficients(38);
        coeffs.extend([(1_u128 << 127) - 1, (1_u128 << 127) - 2]);
        for coeff in coeffs {
            for exp in [
                lo - 100,
                lo - 40,
                lo - 39,
                lo - 38,
                lo - 1,
                lo,
                lo + 1,
                0,
                hi - 1,
                hi,
                hi + 1,
                hi + p as i32,
            ] {
                for neg in [false, true] {
                    for mode in MODES {
                        let w = Wide {
                            neg,
                            coeff,
                            exp: i64::from(exp),
                            sticky: false,
                        };
                        assert_eq!(
                            fit(w, i64::from(exp), mode).map(finite),
                            narrow(Finite { neg, coeff, exp }, format, mode, &powers),
                            "coeff={coeff}, exp={exp}, mode={mode:?}, P={p}"
                        );
                        if coeff >= 10_u128.pow(p - 1) && coeff < 10_u128.pow(37) {
                            for tail in [1, 5, 9] {
                                let w = Wide {
                                    neg,
                                    coeff,
                                    exp: i64::from(exp),
                                    sticky: true,
                                };
                                let rational = Finite {
                                    neg,
                                    coeff: coeff * 10 + tail,
                                    exp: exp - 1,
                                };
                                assert_eq!(
                                    fit(w, i64::from(exp), mode).map(finite),
                                    narrow(rational, format, mode, &powers),
                                    "sticky coeff={coeff}, exp={exp}, tail={tail}, mode={mode:?}, P={p}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn fit_preferred_exponent_clamps_and_extreme_i64_exponents() {
    for (lay, fit) in WIDTHS {
        for neg in [false, true] {
            for pref in [
                i64::MIN,
                i64::from(lay.qmin) - 1,
                0,
                i64::from(lay.qmax) + 1,
                i64::MAX,
            ] {
                let zero = fit(
                    Wide {
                        neg,
                        coeff: 0,
                        exp: i64::MIN,
                        sticky: false,
                    },
                    pref,
                    None,
                )
                .unwrap();
                assert_eq!(
                    finite(zero),
                    Finite {
                        neg,
                        coeff: 0,
                        exp: pref.clamp(i64::from(lay.qmin), i64::from(lay.qmax)) as i32
                    }
                );
            }
            let v = fit(
                Wide {
                    neg,
                    coeff: 10,
                    exp: i64::from(lay.qmin) - 1,
                    sticky: false,
                },
                i64::from(lay.qmin) - 1,
                None,
            )
            .unwrap();
            assert_eq!(
                finite(v),
                Finite {
                    neg,
                    coeff: 1,
                    exp: lay.qmin
                }
            );
            let v = fit(
                Wide {
                    neg,
                    coeff: 1,
                    exp: 0,
                    sticky: false,
                },
                i64::MIN,
                None,
            )
            .unwrap();
            assert_eq!(
                finite(v),
                Finite {
                    neg,
                    coeff: 10_u128.pow(lay.digits - 1),
                    exp: 1 - lay.digits as i32
                }
            );
            for mode in MODES {
                assert_eq!(
                    fit(
                        Wide {
                            neg,
                            coeff: 1,
                            exp: i64::MAX,
                            sticky: false
                        },
                        i64::MAX,
                        mode
                    )
                    .map(finite),
                    Err(Error::Overflow)
                );
                let low = fit(
                    Wide {
                        neg,
                        coeff: 1,
                        exp: i64::MIN,
                        sticky: false,
                    },
                    i64::MIN,
                    mode,
                )
                .map(finite);
                let expected = match mode {
                    None => Err(Error::Inexact),
                    Some(dir) => Ok(Finite {
                        neg,
                        coeff: u128::from((dir == Rounding::TowardPositive) != neg),
                        exp: lay.qmin,
                    }),
                };
                assert_eq!(low, expected);
            }
        }
    }
}
