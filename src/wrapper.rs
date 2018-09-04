// need to generate the pattern cb($self, A:A, B:B ...)
// If we could transform/concat the ident then this would not be needed
#![allow(non_snake_case)]
#![allow(dead_code)]

fn nofun() { }

macro_rules! gen_wrapper {
    ($name:ident ; $($t:ident),*) => {
        pub struct $name<$($t,)*> { before: fn(), after: fn(), func: fn($($t,)*) }
        impl<$($t,)*> $name<$($t,)*> {
            pub fn mk(func:fn($($t,)*)) -> $name<$($t,)*> {
                return $name { before: nofun, after: nofun, func: func }
            }
            pub fn wrap(before: fn(), after: fn(), func: fn($($t,)*)) -> $name<$($t,)*> {
                return $name { before: before, after: after, func: func }
            }
            pub fn cb(&self, $($t:$t,)*) {
                (self.before)();
                (self.func)($($t,)*);
                (self.after)();
            }
        }
    }
}
gen_wrapper!(Callback0 ; );
gen_wrapper!(Callback1 ; A);
gen_wrapper!(Callback2 ; A,B);
gen_wrapper!(Callback3 ; A,B,C);
gen_wrapper!(Callback4 ; A,B,C,D);
gen_wrapper!(Callback5 ; A,B,C,D,E);
gen_wrapper!(Callback6 ; A,B,C,D,E,F);
gen_wrapper!(Callback7 ; A,B,C,D,E,F,G);
gen_wrapper!(Callback8 ; A,B,C,D,E,F,G,H);


pub struct Wrapper {
    before: fn(),
    after: fn(),
}

macro_rules! gen_wrapf {
    ($name:ident ; $cb:ident ; $($t:ident),*) => {
        pub fn $name<$($t,)*>(&self, func: fn($($t,)*)) -> $cb<$($t,)*> {
            return $cb::wrap(self.before, self.after, func);
        }
    }
}
impl Wrapper {
    pub fn mk(before: fn(), after: fn()) -> Wrapper {
        return Wrapper { before: before, after: after }
    }
    gen_wrapf!(f0 ; Callback0 ; );
    gen_wrapf!(f1 ; Callback1 ; A);
    gen_wrapf!(f2 ; Callback2 ; A,B);
    gen_wrapf!(f3 ; Callback3 ; A,B,C);
    gen_wrapf!(f4 ; Callback4 ; A,B,C,D);
    gen_wrapf!(f5 ; Callback5 ; A,B,C,D,E);
    gen_wrapf!(f6 ; Callback6 ; A,B,C,D,E,F);
    gen_wrapf!(f7 ; Callback7 ; A,B,C,D,E,F,G);
    gen_wrapf!(f8 ; Callback8 ; A,B,C,D,E,F,G,H);
}