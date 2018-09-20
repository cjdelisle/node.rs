extern crate mio;
extern crate mio_extras;
extern crate tooples;
extern crate bytes;

pub mod node;
pub mod callback;
pub mod time;
pub mod dgram;
#[macro_use] pub mod macros;

#[cfg(test)]
mod tests {
    use mio;
    use node::{ Loop, module };
    use time::{ set_timeout, set_interval, clear_timeout };

    struct MyObj {
        i: u32
    }
    impl MyObj {
        fn object_method(&mut self) {
            println!("Hello {}", self.i);
            self.i += 1;
        }
    }

    fn my_function<X:Loop<X>>(_x: &mut X)
    {
        println!("Plain old function");
    }
/*
    #[test]
    fn test_udp() {
        module().run((),|s| {
            s.module().new_thread(true).run(rec!{
                timeout: mio::Token(0),
                counter: 0
            },|s|{

                s.timeout = set_interval(s,|s|{
                    s.counter += 1;
                    println!(">s.i = {}", s.counter);
                    if s.counter > 100 {
                        clear_timeout(s, s.timeout);
                    }
                }, 100);


            })
        });
    }
*/

    #[test]
    fn test_main() {
        println!("hi");
        module().run((1,2,3),|s| {
            println!("here {}", s.2);
            set_timeout(s,|_s|{ println!("Hello1"); }, 100);

            s.with_scope(rec!{
                i: 0,
                x: mio::Token(0)
            },|s|{
                s.i = 300;
                set_timeout(s,|s|{
                    println!("Hello world! {}", s.i);
                    s.i += 1;
                }, 150);
                set_timeout(s,|s|{
                    println!("Hello again! {}", s.i);
                    s.i += 1;
                }, 160);

                s.with_scope(MyObj {
                    i: 33
                },|s|{
                    set_timeout(s,|s|{ s.object_method() }, 500);
                    println!("s.i = {}", s.i);
                    println!("s.p().i = {}", s.p().i);
                });

                s.x = set_interval(s, my_function, 50);
                set_timeout(s,|s|{ clear_timeout(s, s.x); }, 1000);

                s.module().new_thread(true).run(rec!{
                    i: 21,
                    x: mio::Token(0)
                },|s|{
                    println!("Hi hi {}", s.i);
                    s.x = set_interval(s,|s|{
                        s.i += 1;
                        println!("s.i = {}", s.i);
                        if s.i > 100 {
                            clear_timeout(s,s.x);
                        }
                    }, 100);
                })
            });
        });
    }
}