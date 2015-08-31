#![feature(drain)]
#![feature(plugin)]
#![plugin(docopt_macros)]

extern crate hyper;
extern crate docopt;
extern crate rustc_serialize;
extern crate time;

use std::io::Read;
use std::thread;
use std::cmp;
use std::sync::{Arc, Mutex};
use docopt::Docopt;

use hyper::Client;
use hyper::header::Connection;


docopt!(Args, "\
Usage: hyperspank [options] <target>

Options:
    -k, --keep-alive  Keep connection alive between bursts
    -c, --control-thread  Use an aggressive control thread to simulate a single hyperactive client
    -t <thread_count>, --thread-count <thread_count>  The number of threads (not including a control thread) [default: 4]
    -r <requests_per_thread>, --requests-per-thread <requests_per_thread>  The number of requests per thread (may be split over a number of bursts) [default: 100]
    -d <delay_duration>, --delay-duration <delay_duration>  The delay (in milliseconds) between requests on a thread [default: 0]
    -b <burst_size>, --burst-size <burst_size>  The number of requests a thread will send before a delay [default: 1]
    -p <print_on_iteration>, --print-on-iteration <print_on_iteration>  The number of iterations before progress is echoed to the console [default: 1]
",
flag_requests_per_thread: u32,
flag_burst_size: u32,
flag_delay_duration: u32,
flag_print_on_iteration: u32,
flag_thread_count: u32
);


fn main() {
    let args: Args = Args::docopt().decode().unwrap_or_else(|e| e.exit());

    let mut thread_vec = Vec::new();
    let control_running = Arc::new(Mutex::new(true));
    let child_control_running = control_running.clone();
    let mut control_handle: Option<thread::JoinHandle<()>>  = None;

    if args.flag_control_thread {
        let tgt = args.arg_target.clone();
        let keep_alive = args.flag_keep_alive;

        control_handle = Some(thread::spawn(move || {
            let client = Client::new();
            let mut iteration = 1;

            while *child_control_running.lock().unwrap() {
                if inner_spank("CONTROL", &keep_alive, &client, &*tgt) {
                    if iteration % 10 == 0 {
                        println!("Thread CONTROL {} - Repeition {}", spank_time(), iteration);
                    }
                } else {
                    println!("Thread CONTROL had an error and broke from its burst on iteration {}", iteration);
                }

                iteration += 1;
            }

            println!("Control thread closed after {} repetitions", iteration);
        }))
    }

    for thread in 0..args.flag_thread_count {
        let tgt = args.arg_target.clone();
        let keep_alive = args.flag_keep_alive;
        let requests_per_thread = args.flag_requests_per_thread;
        let burst_size = args.flag_burst_size;
        let delay_duration = args.flag_delay_duration;
        let print_on_iteration = args.flag_print_on_iteration;

        thread_vec.push(thread::spawn(move || { spank(
            &*format!("{}", thread), &*tgt, keep_alive, requests_per_thread, burst_size, delay_duration, print_on_iteration)
            }));
    }

    for worker in thread_vec.drain(..) {
        match worker.join() {
            Err(e) => { println!("Failed to join thread: {:?}", e) },
            Ok(_) => { /*println!("Joined thread: {:?}", v)*/ }
        }
    }

    match control_handle {
        Some(jh) => {
            println!("Closing control thread...");
            *control_running.lock().unwrap() = false;
            match jh.join() {
                Err(e) => { println!("Failed to join CONTROL thread: {:?}", e) },
                Ok(_) => {}
            }
        },
        None => {}
    }
}


fn spank_time() -> String {
    format!("{}", time::strftime("%Y-%m-%dT%H:%M:%f", &time::now()).ok().unwrap())
}

fn inner_spank(my_name: &str, keep_alive: &bool, client: &Client, target: &str) -> bool {
    let mut smooth_sailing = true;
    let mut kah = Connection::close();
    let mut body = String::new();

    if *keep_alive {
        kah = Connection::keep_alive();
    }

    match client.get(target).header(kah).send() {
        Err(e) => {
            println!("Thread {} {} - Get failed: {:?}", my_name, spank_time(), e);
            smooth_sailing = false;
            },
        Ok(mut r) => match r.read_to_string(&mut body) {
            Err(e) => {
                println!("Thread {} {} - Read to string failed: {:?}", my_name, spank_time(), e);
                smooth_sailing = false;
                },
            Ok(_) => { }
        }
    }

    smooth_sailing
}

fn spank(my_name: &str, target: &str, keep_alive: bool, reqs: u32, burst_size: u32, delay_duration: u32, print_on_iteration: u32) {
    let client = Client::new();

    let mut reqs_left = reqs;
    let mut smooth_sailing = true;

    while reqs_left > 0 {
        let mut burst_reqs_left = cmp::min(burst_size, reqs_left);
        if burst_size > 1 {
            println!("Thread {} - Starting new burst of {} requests", my_name, burst_size);
        }

        while burst_reqs_left > 0 {
            smooth_sailing = inner_spank(my_name, &keep_alive, &client, target);

            let iteration = (reqs - reqs_left) + 1;

            if iteration % print_on_iteration == 0 {
                println!("Thread {} {} - Repetition {}", my_name, spank_time(), iteration);
            }

            burst_reqs_left -= 1;
            reqs_left -= 1;

            if !smooth_sailing {
                println!("Thread {} had an error and broke from its burst on iteration {}", my_name, iteration);
                break;
            }
        }

        if smooth_sailing && reqs_left > 0  && delay_duration > 0 {
            println!("Thread {} sleeping...", my_name);
            thread::sleep_ms(delay_duration);
        } else {
            smooth_sailing = true;
        }
    }
}
