#[macro_export] macro_rules! rec {
    (@mkstruct (
        [$firstlet:ident, $($letter:ident),*],
        { }
    ) -> ([$($letter_out:ident),*], {$($id_out:ident : $val_out:expr),*})) => {
        {
            struct Rec<$($letter_out,)*> { $($id_out : $letter_out),* }
            Rec { $($id_out : $val_out),* }
        }
    };
    (@mkstruct (
        [ $letter0:ident, $($letter:ident),* ],
        { $id0:ident : $val0:expr $(,$id:ident : $val:expr)* }
    ) -> ([ $($letter_out:ident),* ], { $($id_out:ident : $val_out:expr),* })) => {
        rec!(@mkstruct([$($letter),*], { $($id : $val),* }) -> (
            [$letter0 $(,$letter_out)*],
            { $id0 : $val0 $(,$id_out : $val_out)* }
        ));
    };
    { $($id:ident : $val:expr),+ } => {
        rec!(@mkstruct([A,B,C,D,E,F,G,H,I,J,K,L], { $($id : $val),* } ) -> ([],{}) );
    }
}