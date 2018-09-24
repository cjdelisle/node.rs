# node.rs

A nodejs-like framework for Rust.

The biggest thing that makes nodejs pleasent to program in is the API. Even the most ardent Javascript
fan will agree that Js has a certain amount of [wat](https://www.destroyallsoftware.com/talks/wat)
inducing design decisions, but what's important about javascript, and especially nodejs, is that
the code written in it usually Just Works. There isn't a complex type system (or any type system) to
impede quick development and the APIs strike a good balance between making it *easy* to do what people
typically want to do, and avoiding "magic" commands, which are difficult to understand and reason
about, or worse, which the API designed expects the consumer to invoke blindly.


## Why API matters

Here's an example of why API matters so much. This is a little program which reads out a file and
replaces all instances of the word "cloud" (case insensitive) with the word "butt". Our first example
showcases all the terrible API of the Java standard library:

```java
import java.util.regex.Pattern;
import java.io.BufferedReader;
import java.io.FileReader;
import java.io.IOException;

public class ApiMatters
{
    static final Pattern REGEX = Pattern.compile("cloud", Pattern.CASE_INSENSITIVE);
    public static void main(String[] args) throws IOException {
        BufferedReader br = new BufferedReader(new FileReader("file.txt"));
        try {
            String line = br.readLine();
            while (line != null) {
                System.out.println(REGEX.matcher(line).replaceAll("butt"));
                line = br.readLine();
            }
        } finally {
            br.close();
        }
    }
}
```

Java really wants you to know *how* things are happening, every little detail is necessary, you need
to know that you're reading a File, with a FileReader, and in case you might not want to cause a syscall
every time you read, well you need a BufferedReader. And doing regex replacement requires that you create
a Pattern object, which you should put as a static field in the class of your Object, you are making an
Object, right ?

At the other extreme you have magic, the best example I can think of is in the old Bitcoin codebase, when
the data structures are to be read or written to/from the disk or network, they need to be converted into
a serialized form. Satoshi's lovely solution for this is one magic macro:
[IMPLEMENT_SERIALIZE](https://github.com/bitcoin/bitcoin/blob/v0.8.6/src/protocol.h#L42) which expands to
about [1000 lines of nested C++ macro code](https://github.com/bitcoin/bitcoin/blob/v0.8.6/src/serialize.h).
Satoshi to his credit only used this monstrosity for his own code. The real offense is exporting these
[magical constructs as API](https://blogs.msdn.microsoft.com/oldnewthing/20050106-00/?p=36783/).

Magic APIs writers don't try to explain what's happening, instead they write documentation which amounts
to "just call it, don't ask too many questions, it will work". But if ever it doesn't work, you're gonna
be in a world of pain trying to understand what that thousand lines of meta-meta-meta-polymorphic
programming actually does. The fundimental problem with magical APIs is they lack a solid metaphore, you
really don't know if switching on the lights might cause the toilet to flush, and if it does, whether that
is a bug, or some Rube Goldberg "feature" to save the mad scientist API author.

Nodejs is pleasent to work with because the API creators  took the middle road. Javascript doesn't
(as of 2018) have powerful macros so creating magical APIs is not easy, and the creators of nodejs made
a solid effort to avoid
[global flags](https://softwareengineering.stackexchange.com/questions/173086/are-flag-variables-an-absolute-evil)
and
[side effects](http://codebetter.com/matthewpodwysocki/2008/04/30/side-effecting-functions-are-code-smells/)
in their API design, while still hiding most of the things which the typical programmer is not likely to
care about.

Take this snippet as example:

```js
const Fs = require('fs');
Fs.readFile('./file.txt', 'utf8', (err, data) => {
    if (err) { throw err; }
    data.split('\n').forEach((l) => {
        console.log(l.replace(/cloud/i, 'butt'));
    })
});
```

Like the big Java example, it also reads the file, replaces cloud with butt and writes out the result.
Unlike the big Java example, it doesn't require the programmer to know about buffer, regular expression
compiling or any of the many types of errors which can occur throughout the process. A fair criticism of
nodejs is that it is not easy to verify that all exceptional cases have been handled, but when you're
trying to get a project off the ground, handling all exceptional cases is the least of your concerns.

## What is Rust

Rust is a compiled language, so
like C/C++ it can make small fast standalone binaries. Rust is also a memory safe language, so like
Javascript, it cannot segfault[*](https://doc.rust-lang.org/nomicon/meet-safe-and-unsafe.html). Rust's
type system is a state of the art system,
[comparable to that of a function language like haskell](https://sdleffler.github.io/RustTypeSystemTuringComplete/)
but Rust itself is procedural, often resembling C++. In some ways, you could imagine Rust as two
languages, the procedural language which you use to write the code and the functional language which
you use to convince the type system that your code is safe.

## Introducing node.rs

Node.rs is an attempt at bringing the good stuff from Nodejs to Rust. It is built on top of
[MIO](https://github.com/carllerche/mio) and contains an embedded event loop and callback functionality.

### Simple example

The most simple example is a `setTimeout()`, unlike nodejs, you need to launch the event loop explicitly,
you do that with the module builder. After building the module, you are called with a [Scope](#scope).
The Scope provides you access to the underlying event loop and is the first argument which is passed to
every callback that is called. We'll get to the module builder later on, but for now you can just wrap
your program with `module().run((), |s| {` ..... `});`.

```rust
use node_rs::module;
use node_rs::time::set_timeout;
fn main() {
    module().run((), |s| {
        set_timeout(s, |s, _| {
            println!("Hello1");
        }, 100);
    });
}
```

### The Scope

In Javascript and other Scheme-like languages, nested scopes can access and modify variables of
parent scopes like this:

```js
let sum = 0;
[1,2,3,4,5].forEach((i) => {
    sum += i
});
console.log(sum);
```

This example is rather simple because everything happens synchronously. The number `sum` is gone
by the time the code snippet completes.

However, in this example:
```js
let x = 0;
setTimeout(() => { x++; }, 100);
setTimeout(() => { console.log(x); }, 200);
```

The number, `x` needs to continue to exist after the function where it was declared returns.
Javascript achieves this by means of single-threaded execution and garbage collection, because two
closures have been registered to the `setTimeout` function and those two closures hold a reference
to `x`, Javascript will keep the memory location for `x` in memory until they are complete.

Because Rust has no garbage collector, every object in memory must have a unique *owner*,
furthermore, in order to avoid [pointer aliasing](https://en.wikipedia.org/wiki/Pointer_aliasing)
issues, the Rust language rules specify that if there is a mutable pointer to an object, there
cannot be any other pointer to the same object at the same time.

So in Rust, the first example works:

```rust
fn main() {
    let mut sum = 0;
    vec![1,2,3,4,5].iter().for_each(|i|{
        sum += i
    });
    println!("{}", sum);
}
```

But the second example fails, because `i` is *owned* by the main function, and so it is de-allocated
when the main function completes.

```rust
// error[E0597]: `i` does not live long enough
fn main() {
    let fake_event_loop = || {
        let mut i = 0;
        return (
            || { i += 1; },
            || { println!("{}", i); }
        );
    };
    let mut callbacks = fake_event_loop();
    (callbacks.0)();
    (callbacks.1)();
}
```
[Try it out in the Rust Playground](https://play.rust-lang.org/?gist=f49f4eadcf16b568c53f5b1ce12f7fd9&version=stable&mode=debug&edition=2015)

### How the Scope works

When you created a module with `module().run((), |s| {` ..... `});`, you might have noticed that
the first argument to `run()` is `()`, the [unit](https://doc.rust-lang.org/std/primitive.unit.html)
(similar to `null` in other programming languages). This object can be anything you want to pass in,
and the scope (`s`) will be created from that object. For example:
