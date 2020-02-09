use std::convert::From;
use std::fmt::{self, Display, Write};
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

use num_bigint::BigInt;
use num_rational::BigRational;

const PRECISION: usize = 10;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct Number {
    val: BigRational,
}

impl Number {
    pub const fn new(val: BigRational) -> Number {
        Number { val }
    }

    pub fn to_integer(&self) -> BigInt {
        self.val.to_integer()
    }

    pub fn ratio<A: Into<BigInt>, B: Into<BigInt>>(a: A, b: B) -> Self {
        Number::new(BigRational::new(a.into(), b.into()))
    }

    pub fn round(self) -> Self {
        Number {
            val: self.val.round(),
        }
    }

    pub fn ceil(self) -> Self {
        Number {
            val: self.val.ceil(),
        }
    }

    pub fn floor(self) -> Self {
        Number {
            val: self.val.floor(),
        }
    }
}

impl fmt::LowerHex for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:0>2x}", self.val.to_integer())
    }
}

impl From<BigInt> for Number {
    fn from(b: BigInt) -> Self {
        Number {
            val: BigRational::from_integer(b),
        }
    }
}

macro_rules! from_integer {
    ($ty:ty) => {
        impl From<$ty> for Number {
            fn from(b: $ty) -> Self {
                Number {
                    val: BigRational::from_integer(BigInt::from(b)),
                }
            }
        }
    };
}

from_integer!(u16);
from_integer!(usize);
from_integer!(i32);
from_integer!(u8);

impl Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.val.to_integer())?;
        let mut frac = self.val.fract();
        if frac != BigRational::from_integer(BigInt::from(0)) {
            f.write_char('.')?;
            for _ in 0..PRECISION {
                frac *= BigRational::from_integer(BigInt::from(10));
                write!(f, "{}", frac.to_integer())?;
                frac = frac.fract();
                if frac == BigRational::from_integer(BigInt::from(0)) {
                    break;
                }
            }
            if frac != BigRational::from_integer(BigInt::from(0)) {
                write!(
                    f,
                    "{}",
                    (frac * BigRational::from_integer(BigInt::from(10)))
                        .round()
                        .to_integer()
                )?;
            }
        }
        Ok(())
    }
}

impl Add for Number {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Number {
            val: self.val + other.val,
        }
    }
}

impl AddAssign for Number {
    fn add_assign(&mut self, other: Self) {
        self.val += other.val
    }
}

impl Sub for Number {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Number {
            val: self.val - other.val,
        }
    }
}

impl SubAssign for Number {
    fn sub_assign(&mut self, other: Self) {
        self.val -= other.val
    }
}

impl Mul for Number {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        Number {
            val: self.val * other.val,
        }
    }
}

impl MulAssign for Number {
    fn mul_assign(&mut self, other: Self) {
        self.val *= other.val
    }
}

impl Div for Number {
    type Output = Self;

    fn div(self, other: Self) -> Self {
        Number {
            val: self.val / other.val,
        }
    }
}

impl DivAssign for Number {
    fn div_assign(&mut self, other: Self) {
        self.val /= other.val
    }
}
