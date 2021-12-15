use crate::bus::{Bus, IO};
use crate::clock::Clock;
use crate::cpu::Cpu;
use crate::crc16::Crc16;
use crate::error::Error;
use crate::screen::Screen;

// TODO: rename
const PAD_MAPPINGS: [(char, usize); 0x10] = [
    ('1', 0x1),
    ('2', 0x2),
    ('3', 0x3),
    ('4', 0xc),
    ('q', 0x4),
    ('w', 0x5),
    ('e', 0x6),
    ('r', 0xd),
    ('a', 0x7),
    ('s', 0x8),
    ('d', 0x9),
    ('f', 0xe),
    ('z', 0xa),
    ('x', 0x0),
    ('c', 0xb),
    ('v', 0xf),
];
const FONT_SPRITES: [[u8; 5]; 0x10] = [
    [0b11110000, 0b10010000, 0b10010000, 0b10010000, 0b11110000],
    [0b00100000, 0b01100000, 0b00100000, 0b00100000, 0b01110000],
    [0b11110000, 0b00010000, 0b11110000, 0b10000000, 0b11110000],
    [0b11110000, 0b00010000, 0b11110000, 0b00010000, 0b11110000],
    [0b10010000, 0b10010000, 0b11110000, 0b00010000, 0b00010000],
    [0b11110000, 0b10000000, 0b11110000, 0b00010000, 0b11110000],
    [0b11110000, 0b10000000, 0b11110000, 0b10010000, 0b11110000],
    [0b11110000, 0b00010000, 0b00100000, 0b01000000, 0b01000000],
    [0b11110000, 0b10010000, 0b11110000, 0b10010000, 0b11110000],
    [0b11110000, 0b10010000, 0b11110000, 0b00010000, 0b11110000],
    [0b11110000, 0b10010000, 0b11110000, 0b10010000, 0b10010000],
    [0b11100000, 0b10010000, 0b11100000, 0b10010000, 0b11100000],
    [0b11110000, 0b10000000, 0b10000000, 0b10000000, 0b11110000],
    [0b11100000, 0b10010000, 0b10010000, 0b10010000, 0b11100000],
    [0b11110000, 0b10000000, 0b11110000, 0b10000000, 0b11110000],
    [0b11110000, 0b10000000, 0b11110000, 0b10000000, 0b10000000],
];
const SCREEN_SIZE: (usize, usize) = (64, 32);
const PROGRAM_START: u16 = 0x0200;
const FONT_OFFSET: u16 = 0x0000;
const CPU_FREQUENCY: f32 = 500.0;
const TIMER_FREQUENCY: f32 = 60.0;
const RNG_SEED: u16 = 0xcafe;

#[derive(Debug)]
pub struct Chip8 {
    cpu: Cpu,
    bus: Bus,
    mapping: [char; PAD_MAPPINGS.len()],
    screen_size: (usize, usize),
    clock_60htz: Clock,
    clock_cpu: Clock,
}

impl Chip8 {
    pub fn new(freq: Option<f32>) -> Self {
        let freq = freq.unwrap_or(CPU_FREQUENCY);

        let mut sorted_map: Vec<_> = PAD_MAPPINGS.into();
        sorted_map.sort_by_key(|mapping| mapping.1);

        let mut mapping = [Default::default(); PAD_MAPPINGS.len()];
        mapping
            .iter_mut()
            .zip(sorted_map.into_iter().map(|(key, _)| key))
            .for_each(|(dst, src)| *dst = src);

        Self {
            cpu: Default::default(),
            bus: Default::default(),
            mapping,
            screen_size: SCREEN_SIZE,
            clock_60htz: Clock::new(std::time::Duration::from_secs(1).div_f32(TIMER_FREQUENCY)),
            clock_cpu: Clock::new(std::time::Duration::from_secs(1).div_f32(freq)),
        }
    }

    pub fn load_rom(&mut self, rom: &[u8], seed: Option<u16>) -> Result<(), Error> {
        let ft = FONT_OFFSET;
        let pc = PROGRAM_START;

        // Copy sprites in memory
        FONT_SPRITES.iter().try_fold(ft, |addr, sprite| {
            sprite.iter().copied().try_fold(addr, |addr, byte| {
                self.bus.ram.write(addr, byte)?;
                Ok(addr.wrapping_add(1))
            })
        })?;

        // Copy ROM in memory
        let mut crc = Crc16::start();

        rom.iter().copied().try_fold(pc, |addr, byte| {
            crc.update(byte);
            self.bus.ram.write(addr, byte)?;
            Ok(addr.wrapping_add(1))
        })?;

        // Derive seed from ROM if not provided
        let seed = seed.unwrap_or_else(|| match crc.finish() {
            0x0000 => RNG_SEED,
            n => n,
        });

        self.cpu.init(pc, ft);
        self.bus.rng.seed(seed);

        Ok(())
    }

    pub fn clock(&mut self, screen: &mut [bool], pad: &[bool], audio: &mut bool) -> Result<(), Error> {
        if screen.len() != (self.screen_size.0 * self.screen_size.1) {
            return Err(Error::InvalidScreenSize(
                self.screen_size.0 * self.screen_size.1,
                screen.len(),
            ));
        }

        if pad.len() != self.mapping.len() {
            return Err(Error::InvalidPadSize(self.mapping.len(), pad.len()));
        }

        let mut io = IO {
            screen: Screen {
                memory: screen,
                width: self.screen_size.0,
                height: self.screen_size.1,
            },
            pad,
            audio,
        };

        self.clock_cpu.tick(std::time::Instant::now(), || {
            self.cpu.cycle(&mut self.bus, &mut io)?;
            Ok(())
        })?;

        self.clock_60htz.tick(std::time::Instant::now(), || {
            self.bus.dt.clock();
            self.bus.st.clock();
            Ok(())
        })?;

        *audio = self.bus.st.get() > 0;

        Ok(())
    }

    pub fn get_mapping(&self) -> &[char] {
        &self.mapping
    }

    pub fn get_screen_size(&self) -> (usize, usize) {
        self.screen_size
    }
}
