#[derive(Clone, Copy)]
pub(crate) enum Format {
    D32,
    D64,
    D128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Finite {
    pub(crate) neg: bool,
    pub(crate) coeff: u128,
    pub(crate) exp: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Classified {
    Finite(Finite),
    NonCanonical,
    Infinity,
    Nan,
}

impl Format {
    pub(crate) const fn parameters(self) -> (u32, u32, u32, i32, u128) {
        match self {
            Self::D32 => (32, 6, 20, 101, 9_999_999),
            Self::D64 => (64, 8, 50, 398, 9_999_999_999_999_999),
            Self::D128 => (
                128,
                12,
                110,
                6176,
                9_999_999_999_999_999_999_999_999_999_999_999,
            ),
        }
    }
}

// IEEE 754-2019 3.5.2: split S, G and T first, then reconstruct E and C.
// Non-canonical significands are classified separately, never replaced by zero.
pub(crate) fn classify(bits: u128, format: Format) -> Classified {
    let (k, w, t, bias, max_coeff) = format.parameters();
    let s = bits >> (k - 1);
    let g = (bits >> t) & ((1 << (w + 5)) - 1);
    let trailing = bits & ((1 << t) - 1);
    let (e, leading) = match g >> w {
        30 => return Classified::Infinity,
        31 => return Classified::Nan,
        0..=23 => (g >> 3, g & 7),
        24..=29 => ((g >> 1) & ((1 << (w + 2)) - 1), 8 + (g & 1)),
        _ => unreachable!(),
    };
    let c = (leading << t) + trailing;
    if c > max_coeff {
        Classified::NonCanonical
    } else {
        Classified::Finite(Finite {
            neg: s != 0,
            coeff: c,
            exp: e as i32 - bias,
        })
    }
}

pub(crate) fn encode(value: Finite, format: Format) -> u128 {
    let (k, w, t, bias, max_coeff) = format.parameters();
    assert!(value.coeff <= max_coeff);
    let e = (value.exp + bias) as u128;
    assert!(e < 3 << w);
    let leading = value.coeff >> t;
    let g = if leading < 8 {
        (e << 3) + leading
    } else {
        (3 << (w + 3)) + (e << 1) + (leading - 8)
    };
    ((value.neg as u128) << (k - 1)) + (g << t) + (value.coeff & ((1 << t) - 1))
}
