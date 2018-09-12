extern crate mio;
extern crate mio_extras;

pub mod node;
pub mod callback;

#[cfg(test)]
mod tests {
    use callback::cb0;
    use mio;
    use node::{ Loop, module };

    struct X {
        i: u32,
    }
    impl X {
        fn hi(&self) {
            println!("Hello {}", self.i);
        }
    }

    fn test<X:Loop<X>>(_x: &mut X)
    {
        println!("Plain old function");
    }
    struct ABC {
        i: i32,
        x: mio::Token
    }

    #[test]
    fn test_main()
    {
        println!("hi");
        module().run((1,2,3),|s| {
            println!("here {}", s.2);
            s.core().set_timeout(cb0(s,|_s|{ println!("Hello1"); }), 100, false);

            s.with_scope(ABC {
                i: 0,
                x: mio::Token(0)
            },|s|{
                s.i = 300;
                s.core().set_timeout(cb0(s,|s|{
                    println!("Hello world! {}", s.i);
                    s.i += 1;
                }), 150, false);
                s.core().set_timeout(cb0(s,|s|{
                    println!("Hello again! {}", s.i);
                    s.i += 1;
                }), 160, false);

                s.with_scope(X {
                    i: 33
                },|s|{
                    s.core().set_timeout(cb0(s,|s|{ s.hi() }), 500, false);
                    println!("s.i = {}", s.i);
                    println!("s.p().i = {}", s.p().i);
                });

                s.x = s.core().set_timeout(cb0(s,test), 50, true);
                s.core().set_timeout(cb0(s,|s|{
                    let t = s.x;
                    match s.core().deregister_event(&t) { _=>() }
                }), 1000, false);

                s.module().run(X {
                    i: 21
                },|s|{
                    println!("Hi hi {}", s.i);
                })
            });
        });
    }
}