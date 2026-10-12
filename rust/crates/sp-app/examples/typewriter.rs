//! Hear the typewriter: a line of keys, then the bell, through this
//! computer's default output. Prints whether an output opened.

use std::thread::sleep;
use std::time::Duration;

use sp_app::app::sound::Speaker;

fn main() {
    let Some(mut speaker) = Speaker::open() else {
        println!("no output opened: the window would be quiet");
        return;
    };
    println!("output open");
    for _ in 0..24 {
        speaker.key();
        sleep(Duration::from_millis(70));
    }
    speaker.ding();
    sleep(Duration::from_millis(700));
}
