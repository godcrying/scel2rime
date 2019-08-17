extern crate clap;
extern crate encoding;
extern crate byteorder;

use std::fs::File;
use std::io::prelude::*;
use std::io::Cursor;
use std::io::SeekFrom;
use std::io::ErrorKind;

use std::path::Path;
use std::ffi::OsString;
use std::error;
use std::fmt;

use clap::{App, Arg};
use encoding::all::{UTF_16LE,UTF_8};
use encoding::{Encoding,EncoderTrap,DecoderTrap};
use byteorder::{ByteOrder,LittleEndian,ReadBytesExt};


// 自定义错误类型
#[derive(Debug, Clone)]
struct WrongFileType;

impl fmt::Display for WrongFileType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Invalid file type")
    }
}

impl error::Error for WrongFileType {
    fn description(&self) -> &str {
        "invalid first item to double"
    }

    fn cause(&self) -> Option<&error::Error> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}

struct WordListItem {
    py_index_list: Vec<usize>,
    word: String,
    priority: usize,
}

pub struct Config {
    pub inputfile: String, 
    pub outputfile: String,
}

impl Config {
    pub fn new() -> Result<Config, &'static str> {
        let matches = App::new("scel2rime")
                    .version("0.1.0")
                    .author("godcrying")
                    .about("Convert sogou scel file to rime dict file.")
                    .arg(Arg::with_name("input")
                        .short("i")
                        .long("input")
                        .takes_value(true)
                        .help("A sogou scel filename."))
                    .arg(Arg::with_name("output")
                        .short("o")
                        .long("output")
                        .takes_value(true)
                        .help("An output filename."))
                    .get_matches();

        let input_file = match matches.value_of("input") {
            Some(s) => s.to_string(),
            None => {
                return Err("Can not find a input file!!");
            }
        };

        let output_file = match matches.value_of("output") {
            Some(s) => s.to_string(),
            None => {
                let input_path = Path::new(&input_file);
                let filename = input_path.file_stem().unwrap();
                format!("{}{}",filename.to_str().unwrap(), ".txt")
            }
        };

        Ok(Config{ inputfile: input_file, outputfile: output_file})
    }
}

pub fn run(config:Config) -> Result<(), Box<dyn error::Error>> {

    let pinyin_offset = 0x1540;
    let words_offset = 0x2628;

    let mut infileobj = File::open(&config.inputfile)?;
    let mut outfileobj = File::create(&config.outputfile)?;
    
    let mut data: Vec<u8> = Vec::new();
    infileobj.read_to_end(&mut data)?;

    let file_flag:Vec<u8> = b"\x40\x15\x00\x00\x44\x43\x53\x01\x01\x00\x00\x00".to_vec();
    if file_flag != &data[0..12]{
        eprintln! ("确认你选择的是搜狗(.scel)词库?");
        return Err(WrongFileType.into());
    };

    let pinyin_table = get_pinyin_table(&data[pinyin_offset..words_offset])?;
    let wordlist = get_word_list(&data[words_offset..]);

    println!("词库名称：{}",UTF_16LE.decode(&data[0x130..0x338], DecoderTrap::Strict).unwrap());
    println!("词库类型：{}",UTF_16LE.decode(&data[0x338..0x540], DecoderTrap::Strict).unwrap());
    println!("词库信息：{}",UTF_16LE.decode(&data[0x540..0xd40], DecoderTrap::Strict).unwrap());
    println!("词库示例：{}",UTF_16LE.decode(&data[0xd40..pinyin_offset], DecoderTrap::Strict).unwrap());

    //let mut result:Vec<(String, String)> = Vec::new();

    assert_eq!(String::from("zhuang"), pinyin_table[0x0191]);

    for s in &wordlist {
        let word = &s.word;
        let pinyin_index = &s.py_index_list;
        let mut pinyin = format!("");
        
        for index in pinyin_index {
            pinyin = format!("{} {}",pinyin,pinyin_table[*index]);
        }
        let item = format!("{}\t{}\n",word,pinyin).to_string();
        outfileobj.write(&UTF_8.encode(&item,EncoderTrap::Strict).unwrap()).unwrap();
    }

    Ok(())
}

fn get_pinyin_table(data: &[u8]) -> Result<Vec<String>, Box<dyn error::Error>> {

    let mut py_table: Vec<String> = Vec::new();
    if &data[0..4] != b"\x9D\x01\x00\x00" {
        panic!("No pinyin table found!! File maybe destroied.");
    }

    let mut csr = Cursor::new(&data[4..]);
    loop {
        match csr.read_u16::<LittleEndian>() {
            Ok(_) => {
                let py_len = csr.read_u16::<LittleEndian>()? as usize;
                let pinyinstart = csr.position() as usize;
                let pinyin = UTF_16LE.decode(&csr.get_ref()[pinyinstart..pinyinstart+py_len], DecoderTrap::Strict).unwrap();
                py_table.push(pinyin);
                csr.seek(SeekFrom::Current(py_len as i64)).unwrap();
            },
            Err(e) => {
                match e.kind() {
                    ErrorKind::UnexpectedEof => break,
                    _ => {
                        panic!("Unknown issue occured!!");
                    }
                }
            }
        }
    }
    Ok(py_table)
}

fn get_word_list(data:&[u8]) -> Vec<WordListItem> {
   
    let mut word_list = Vec::new();

    let mut csr = Cursor::new(&data);
    loop {
        match csr.read_u16::<LittleEndian>() {
            Ok(mut same_num) => {
                let mut pinyin: Vec<usize> = Vec::new();
                let py_len = csr.read_u16::<LittleEndian>().unwrap() as usize;
                let current_pos = csr.position() as usize;
                while (csr.position() as usize) < current_pos +py_len {
                    let py = csr.read_u16::<LittleEndian>().unwrap() as usize;
                    pinyin.push(py);
                }

                assert_eq!(csr.position() as usize, current_pos + py_len);

                while same_num > 0 {
                    let word_len = csr.read_u16::<LittleEndian>().unwrap() as usize;
                    let wordstartpos = csr.position() as usize;
                    let word = UTF_16LE.decode(&csr.get_ref()[wordstartpos..wordstartpos+word_len], DecoderTrap::Strict).unwrap();
                    csr.seek(SeekFrom::Current(word_len as i64)).unwrap();
                    let ext_len = csr.read_u16::<LittleEndian>().unwrap() as usize;
                    let priority = csr.read_u16::<LittleEndian>().unwrap() as usize;
                    csr.seek(SeekFrom::Current((ext_len-2) as i64)).unwrap();
                    word_list.push(WordListItem{py_index_list: pinyin.clone(), word: word, priority: priority});
                    same_num -=1;
                }
            },
            Err(e) => {
                match e.kind() {
                    ErrorKind::UnexpectedEof => break,
                    _ => {
                        panic!("Unknown issue occured!!");
                    }
                }
            }
        }
    }
    word_list
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn testPath() {
        let path = Path::new("/home/zhenyu/touhou.scel");
        assert_eq!(path.file_stem().unwrap() , &OsString::from("touhou"));
    }

}