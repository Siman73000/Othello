#![no_std]
#![no_main]

const VIDEO_MEMORY: usize = 0xb8000;
const WHITE_ON_BLACK: u8 = 0x0f;

#[no_mangle]
pub extern "C" fn print32(mut message: *const u8) {
    unsafe {
        let mut video_memory: *mut u16 = VIDEO_MEMORY as *mut u16;

        while *message != 0 {
            let character = *message;
            let attribute = WHITE_ON_BLACK;
            let value: u16 = ((attribute as u16) << 8) | (character as u16);

            // Store character + attribute in video memory
            *video_memory = value;

            message = message.add(1);  // Move to next character
            video_memory = video_memory.add(1); // Move to next video memory position
        }
    }
}
