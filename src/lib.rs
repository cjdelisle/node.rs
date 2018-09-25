extern crate mio;
extern crate mio_extras;
extern crate bytes;

// Same as an mio token, but exported to downstream libraries
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Token (pub usize);
impl Into<mio::Token> for Token { fn into(self) -> mio::Token { mio::Token(self.0) } }
impl From<mio::Token> for Token { fn from(val: mio::Token) -> Token { Token(val.0) } }
impl Into<usize> for Token { fn into(self) -> usize { self.0 } }
impl From<usize> for Token { fn from(val: usize) -> Token { Token(val) } }

#[macro_use] pub mod macros_internal;
#[macro_use] pub mod macros;
pub mod node;
pub mod callback;
pub mod time;
pub mod dgram;

pub fn module() -> node::ModuleCfg { node::module() }

#[cfg(test)]
mod tests2 {
    use node::module;
    use time::set_timeout;

    #[test]
    fn test() {
        module().run((), |s| {
            set_timeout(s,|_,_|{
                println!("Hello1");
            }, 100);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::Token;
    use node::{ Loop, module };
    use time::{ set_timeout, set_interval, clear_timeout };
    use dgram::*;

    struct MyObj {
        i: u32
    }
    impl MyObj {
        fn object_method(&mut self) {
            println!("Hello {}", self.i);
            self.i += 1;
        }
    }

    fn my_function<X:Loop<X>>(_x: &mut X, _:Token)
    {
        println!("Plain old function");
    }

    #[test]
    fn test_udp() {
        const PORT: u16 = 6666;
        module().run((), |s| {
            let sock = create_socket("udp4").unwrap().bind((PORT, "0.0.0.0")).unwrap();
            let sock2 = create_socket("udp4").unwrap().bind("0.0.0.0").unwrap();
            s.with_scope(rec!{
                sock: sock,
                sock2: sock2
            }, |s| {
                s.sock.on_message(s, |s,msg|{
                    println!("Received message! {:?} from {:?}", msg.buf, msg.sa);
                    s.sock.close();
                });
                s.sock2.send_to(s, "Hello world!", (PORT, "127.0.0.1"), |s,_|{
                    s.sock2.close();
                });
            });
        });
    }

    #[test]
    fn test_udp_thread() {
        const PORT: u16 = 6667;
        module().run((), |s| {
            s.module().new_thread(true).run((),|s|{
                let sock = create_socket("udp4").unwrap().bind((PORT, "0.0.0.0")).unwrap();
                s.with_scope(rec!{
                    sock: sock
                }, |s| {
                    s.sock.on_message(s, |s,msg|{
                        println!("Received message! {:?} from {:?}", msg.buf, msg.sa);
                        s.sock.close();
                    });
                });
            });

            s.module().new_thread(true).run((),|s|{
                let sock2 = create_socket("udp4").unwrap().bind("0.0.0.0").unwrap();
                s.with_scope(rec!{
                    sock2: sock2
                }, |s| {
                    s.sock2.send_to(s, "Hello world!", (PORT, "127.0.0.1"), |s,_|{
                        s.sock2.close();
                    });
                });
            });
        });
    }

    #[test]
    fn test_main() {
        println!("hi");
        module().run((1,2,3),|s| {
            println!("here {}", s.2);
            set_timeout(s,|_,_|{ println!("Hello1"); }, 100);

            s.with_scope(rec!{
                i: 0,
                x: Token(0)
            },|s|{
                s.i = 300;
                set_timeout(s,|s,_|{
                    println!("Hello world! {}", s.i);
                    s.i += 1;
                }, 150);
                set_timeout(s,|s,_|{
                    println!("Hello again! {}", s.i);
                    s.i += 1;
                }, 160);

                s.with_scope(MyObj {
                    i: 33
                },|s|{
                    set_timeout(s,|s,_|{ s.object_method() }, 500);
                    println!("s.i = {}", s.i);
                    println!("s.p().i = {}", s.p().i);
                });

                s.x = set_interval(s, my_function, 50);
                set_timeout(s,|s,_|{ clear_timeout(s, s.x); }, 1000);

                s.module().new_thread(true).run(rec!{
                    i: 21,
                    x: Token(0)
                },|s|{
                    println!("Hi hi {}", s.i);
                    s.x = set_interval(s,|s,_|{
                        s.i += 1;
                        println!("s.i = {}", s.i);
                        if s.i > 50 {
                            clear_timeout(s,s.x);
                        }
                    }, 100);
                })
            });
        });
    }
}