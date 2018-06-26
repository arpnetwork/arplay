// Copyright 2018 ARP Network
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern crate ffmpeg_sys as ffmpeg;
extern crate sdl2;

use self::ffmpeg::*;

use self::sdl2::pixels::PixelFormatEnum::IYUV;
use self::sdl2::render::{Texture, WindowCanvas};
use self::sdl2::video::WindowPos;
use self::sdl2::VideoSubsystem;

use std::error::Error;
use std::ptr;
use std::slice;

/// A Window which can draw with planar IYUV pixel data.
struct YUVWindow {
    width: i32,
    height: i32,
    canvas: WindowCanvas,
    texture: Texture,
}

impl YUVWindow {
    /// Constructs a new `YUVWindow`.
    fn new(
        name: &str,
        width: i32,
        height: i32,
        video: VideoSubsystem,
    ) -> Result<YUVWindow, Box<Error>> {
        let canvas = video
            .window(name, width as u32, height as u32)
            .position_centered()
            .build()?
            .into_canvas()
            .build()?;
        let texture =
            canvas
                .texture_creator()
                .create_texture_streaming(IYUV, width as u32, height as u32)?;
        Ok(YUVWindow {
            width,
            height,
            canvas,
            texture,
        })
    }

    /// Updates a rectangle within a planar IYUV texture with new pixel data.
    fn update(&mut self, frame: &YUVFrame) {
        self.texture
            .update_yuv(
                None,
                frame.plane(0),
                frame.pitch(0),
                frame.plane(1),
                frame.pitch(1),
                frame.plane(2),
                frame.pitch(2),
            )
            .unwrap();
        self.canvas.copy(&self.texture, None, None).unwrap();
        self.canvas.present();
    }

    // Hides the window.
    fn hide(&mut self) {
        self.canvas.window_mut().hide();
    }

    /// Sets the new position of the window.
    fn set_position(&mut self, x: i32, y: i32) {
        self.canvas
            .window_mut()
            .set_position(WindowPos::Positioned(x), WindowPos::Positioned(y));
    }
}

struct YUVFrame {
    raw: *mut AVFrame,
}

impl YUVFrame {
    fn new() -> Option<YUVFrame> {
        let raw = unsafe { av_frame_alloc() };
        if raw == ptr::null_mut() {
            return None;
        }

        Some(YUVFrame { raw })
    }

    fn width(&self) -> i32 {
        unsafe { (*self.raw).width }
    }

    fn height(&self) -> i32 {
        unsafe { (*self.raw).height }
    }

    fn plane(&self, index: usize) -> &[u8] {
        unsafe {
            let frame = *self.raw;
            let mut size = (frame.linesize[index] * frame.height) as usize;
            if index > 0 {
                size /= 2;
            }
            slice::from_raw_parts(frame.data[index], size)
        }
    }

    fn pitch(&self, index: usize) -> usize {
        unsafe { (*self.raw).linesize[index] as usize }
    }
}

impl Drop for YUVFrame {
    fn drop(&mut self) {
        unsafe { av_frame_free(&mut self.raw) };
    }
}

/// A Window which can draw with raw H.264 data directly.
pub struct H264Window {
    name: String,
    video: Option<VideoSubsystem>,
    context: *mut AVCodecContext,
    frame: YUVFrame,
    window: Option<YUVWindow>,
}

impl H264Window {
    /// Constructs a new `H264Window`.
    pub fn new(name: &str, video: VideoSubsystem) -> H264Window {
        unsafe {
            let codec = avcodec_find_decoder(AVCodecID::AV_CODEC_ID_H264);
            assert!(codec != ptr::null_mut());
            let context = avcodec_alloc_context3(codec);
            assert!(context != ptr::null_mut());
            let ret = avcodec_open2(context, codec, ptr::null_mut());
            assert!(ret >= 0);

            H264Window {
                name: String::from(name),
                video: Some(video),
                context,
                frame: YUVFrame::new().unwrap(),
                window: None,
            }
        }
    }

    /// Draws canvas with given H.264 data.
    pub fn draw(&mut self, data: &mut [u8]) {
        self.decode(data);
        if self.window.is_none() {
            self.window = YUVWindow::new(
                &self.name,
                self.frame.width(),
                self.frame.height(),
                self.video.take().unwrap(),
            ).ok();
        }
        self.window.as_mut().unwrap().update(&self.frame);
    }

    /// Hides the window.
    pub fn hide(&mut self) {
        self.window.as_mut().unwrap().hide();
    }

    /// Sets the new position of the window.
    pub fn set_position(&mut self, x: i32, y: i32) {
        if let Some(win) = self.window.as_mut() {
            win.set_position(x, y);
        }
    }

    /// Returns the width of the window.
    pub fn width(&self) -> i32 {
        self.size().0
    }

    /// Returns the height of the window.
    pub fn height(&self) -> i32 {
        self.size().1
    }

    /// Returns the size of the window.
    pub fn size(&self) -> (i32, i32) {
        self.window
            .as_ref()
            .and_then(|w| Some((w.width, w.height)))
            .unwrap_or((0, 0))
    }

    /// Returns `true` if the window is shown.
    pub fn is_shown(&self) -> bool {
        self.window.is_some()
    }

    /// Decodes the video frame from data into picture.
    fn decode(&mut self, data: &mut [u8]) {
        unsafe {
            let pkt = av_packet_alloc();
            av_init_packet(pkt);
            av_packet_from_data(pkt, data.as_mut_ptr(), data.len() as i32);
            let ret = avcodec_send_packet(self.context, pkt);
            assert!(ret >= 0);
            let ret = avcodec_receive_frame(self.context, self.frame.raw);
            assert!(ret >= 0);
        }
    }
}

impl Drop for H264Window {
    fn drop(&mut self) {
        unsafe {
            avcodec_free_context(&mut self.context);
        }
    }
}
