macro_rules! debug {
    ($fmt:expr $(,$x:expr)* ) => {
        { let _ = ($($x,)*); }
        //println!(concat!("DEBUG {}:{} ", $fmt), file!(), line!() $(,$x)* );
    }
}
macro_rules! warn {
    ($fmt:expr $(,$x:expr)* ) => {
        { let _ = ($($x,)*); }
        println!(concat!("WARN {}:{} ", $fmt), file!(), line!() $(,$x)* );
    }
}
macro_rules! error {
    ($fmt:expr $(,$x:expr)* ) => {
        { let _ = ($($x,)*); }
        println!(concat!("ERROR {}:{} ", $fmt), file!(), line!() $(,$x)* );
    }
}