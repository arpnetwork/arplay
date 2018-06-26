// Copyright 2018 ARP Network
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern crate arplay;
extern crate bytes;
extern crate sdl2;

use arplay::H264Window;

use bytes::{Buf, IntoBuf};

use sdl2::event::Event;
use sdl2::keyboard::Keycode;

use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::io::AsRawFd;
use std::sync::{mpsc, mpsc::Sender};
use std::thread;
use std::time::{Duration, Instant};
use std::{io, io::prelude::*};

enum Msg {
    New(i32, TcpStream),
    Data(i32, Vec<u8>),
    End(i32),
}

type WindowMap = HashMap<i32, (H264Window, Instant)>;

pub fn main() {
    let sdl = sdl2::init().unwrap();
    let mut windows = HashMap::new();
    let (tx, rx) = mpsc::channel();

    // Gets the size of screen
    let dm = sdl.video().unwrap().current_display_mode(0).unwrap();
    let screen_w = dm.w;
    let screen_h = dm.h;

    spawn_listener(1218, tx.clone());

    let mut event_pump = sdl.event_pump().unwrap();
    'running: loop {
        // Handles GUI events
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,
                _ => {}
            }
        }

        // Handles streaming events
        if let Ok(msg) = rx.recv_timeout(Duration::from_millis(1000 / 60)) {
            match msg {
                Msg::New(fd, s) => {
                    let name = peer_addr(&s);
                    let win = H264Window::new(&name, sdl.video().unwrap());
                    windows.insert(fd, (win, Instant::now()));
                    spawn_streaming(fd, s, tx.clone());
                }
                Msg::Data(fd, mut data) => {
                    let mut is_new = false;
                    if let Some((ref mut win, _)) = windows.get_mut(&fd) {
                        is_new = !win.is_shown();
                        win.draw(data.as_mut());
                    }
                    if is_new {
                        align_windows(&mut windows, screen_w, screen_h);
                    }
                }
                Msg::End(fd) => {
                    windows.remove(&fd).and_then(|(ref mut win, _)| {
                        win.hide();
                        align_windows(&mut windows, screen_w, screen_h);
                        Some(())
                    });
                }
            }
        }
    }
}

/// Spawns a new TCP accept thread.
fn spawn_listener(port: u16, tx: Sender<Msg>) {
    thread::spawn(move || {
        let addr = SocketAddr::new("0.0.0.0".parse().unwrap(), port);
        let listener = TcpListener::bind(addr).unwrap();

        for stream in listener.incoming() {
            if let Ok(s) = stream {
                let fd = s.as_raw_fd();
                tx.send(Msg::New(fd, s)).unwrap();
            }
        }
    });
}

/// Spawns a new TCP streaming thread.
fn spawn_streaming(fd: i32, mut s: TcpStream, tx: Sender<Msg>) {
    thread::spawn(move || loop {
        match read_packet(&mut s) {
            Ok(data) => {
                tx.send(Msg::Data(fd, data)).unwrap();
            }
            _ => {
                tx.send(Msg::End(fd)).unwrap();
                break;
            }
        }
    });
}

/// Returns the socket address of the remote peer of this TCP connection.
fn peer_addr(s: &TcpStream) -> String {
    match s.peer_addr().unwrap() {
        SocketAddr::V4(addr) => format!("{}", addr),
        SocketAddr::V6(addr) => format!("{}", addr),
    }
}

/// Windows padding
const PADDING: i32 = 20;

/// Aligns windows to make it sequence.
fn align_windows(windows: &mut WindowMap, width: i32, height: i32) {
    let size = windows.len() as i32;
    let w = windows
        .iter()
        .fold(0, |acc, (_, (win, _))| acc + win.width()) + (size - 1) * PADDING;
    let mut x = (width - w) / 2;

    // Sorts by created time
    let mut items = Vec::with_capacity(size as usize);
    for (_, item) in windows.iter_mut() {
        items.push(item);
    }
    items.sort_by(|(_, a), (_, b)| a.cmp(&b));

    // Repositions windows
    for (ref mut win, _) in items.iter_mut() {
        let (ww, wh) = win.size();
        win.set_position(x, (height - wh) / 2);
        x += ww + PADDING;
    }
}

/// Reads the packet from stream.
/// |4 bytes size| + |<size> bytes of data|
fn read_packet(s: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut buf = vec![0; 4];
    s.read_exact(&mut buf)?;
    let size = buf.into_buf().get_u32_le() as usize;
    let mut buf = vec![0; size];
    s.read_exact(&mut buf)?;
    Ok(buf)
}
