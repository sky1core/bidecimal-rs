/// An operand of addition, subtraction, multiplication, and division:
/// [`Decimal32`](crate::Decimal32), [`Decimal64`](crate::Decimal64), or
/// [`Decimal128`](crate::Decimal128).
///
/// This trait is sealed and cannot be implemented outside this crate.
pub trait Operand: Copy + Into<crate::Decimal128> + sealed::Sealed {}

pub(crate) mod sealed {
    /// The class of a value, with the fields of a finite value.
    #[derive(Clone, Copy)]
    pub(crate) enum Class {
        Nan { neg: bool, payload: u128 },
        Infinite(bool),
        Finite(crate::layout::Unpacked),
    }

    pub trait Sealed: Copy {
        const IS_NARROW: bool;
        /// Returns `true` for an infinity or NaN.
        fn is_special(self) -> bool;
        /// The fields of a finite value; meaningless for an infinity or NaN.
        #[expect(
            private_interfaces,
            reason = "Sealed cannot be named outside this crate, and type privacy rejects any external call that would produce an Unpacked"
        )]
        fn unpack(self) -> crate::layout::Unpacked;
        #[expect(
            private_interfaces,
            reason = "Sealed cannot be named outside this crate, and type privacy rejects any external call that would produce a Class"
        )]
        fn class(self) -> Class;
    }
}
