use std::{iter, str, thread};

use curl::easy::HttpVersion;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::Result;

use curl::easy::{Easy2, Handler, WriteError};
use curl::multi::{Easy2Handle, Multi};

const NUM_THREADS: usize = 2;

const URL: &str = "http://macmini.local:8080/api/v1/request_info";

const MAX_TOTAL_CONNECTIONS: usize = 16;

const MAX_CONCURRENT_STREAMS: usize = 2000;

const NUM_REQUESTS: usize = 100_000;

struct Collector(Vec<u8>);
impl Handler for Collector {
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        self.0.extend_from_slice(data);
        Ok(data.len())
    }
}

fn download(multi: &mut Multi, token: usize, url: &str) -> Result<Easy2Handle<Collector>> {
    let version = curl::Version::get();
    let mut request = Easy2::new(Collector(Vec::new()));

    request.http_version(HttpVersion::V2PriorKnowledge)?;

    request.url(url)?;

    request.useragent(&format!("curl/{}", version.version()))?;

    let mut handle = multi.add2(request)?;
    handle.set_token(token)?;
    Ok(handle)
}

fn run_thread(thread_number: usize) -> Result<()> {
    println!(
        "thread_number = {} URL = {} MAX_TOTAL_CONNECTIONS = {} MAX_CONCURRENT_STREAMS = {} NUM_REQUESTS = {}",
        thread_number, URL, MAX_TOTAL_CONNECTIONS, MAX_CONCURRENT_STREAMS, NUM_REQUESTS
    );
    let start_time = Instant::now();

    let mut multi = Multi::new();

    multi.set_max_total_connections(MAX_TOTAL_CONNECTIONS)?;

    multi.set_max_concurrent_streams(MAX_CONCURRENT_STREAMS)?;

    let mut handles = iter::repeat(URL)
        .take(NUM_REQUESTS)
        .enumerate()
        .map(|(token, url)| Ok((token, download(&mut multi, token, url)?)))
        .collect::<Result<HashMap<_, _>>>()?;

    println!("handles length = {}", handles.len());

    let mut still_alive = true;
    while still_alive {
        // We still need to process the last messages when
        // `Multi::perform` returns "0".
        if multi.perform()? == 0 {
            still_alive = false;
        }

        multi.messages(|message| {
            let token = message.token().expect("failed to get the token");
            let handle = handles
                .get_mut(&token)
                .expect("the download value should exist in the HashMap");

            match message
                .result_for2(handle)
                .expect("token mismatch with the `EasyHandle`")
            {
                Ok(()) => {
                    let _http_status = handle
                        .response_code()
                        .expect("HTTP request finished without status code");

                    // println!(
                    //     "R: Transfer succeeded (Status: {}) {} (Download length: {})",
                    //     http_status,
                    //     URL,
                    //     handle.get_ref().0.len()
                    // );
                }
                Err(error) => {
                    println!("E: {} - <{}>", error, URL);
                }
            }
        });

        if still_alive {
            // The sleeping time could be reduced to allow other processing.
            // For instance, a thread could check a condition signalling the
            // thread shutdown.
            multi.wait(&mut [], Duration::from_secs(60))?;
        }
    }
    let loop_duration = Instant::now() - start_time;
    println!(
        "after loop loop_duration seconds = {}",
        loop_duration.as_secs_f64()
    );
    let tps = (NUM_REQUESTS as f64) / loop_duration.as_secs_f64();
    println!("tps = {}", tps);

    Ok(())
}

fn main() -> Result<()> {
    println!("begin main");

    let mut thread_join_handles: Vec<thread::JoinHandle<()>> = vec![];

    for i in 0..NUM_THREADS {
        thread_join_handles.push(thread::spawn(move || {
            if let Err(e) = run_thread(i) {
                println!("got run_thread error: {}", e);
            }
        }));
    }

    println!("thread_join_handles.len = {}", thread_join_handles.len());
    for join_handle in thread_join_handles {
        if let Err(e) = join_handle.join() {
            println!("got join_handle.join error: {:?}", e);
        }
    }
    Ok(())
}
