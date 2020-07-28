extern crate clap;
extern crate encoding;
extern crate byteorder;

use std::fs::File;
use std::io::prelude::*;
use std::io::Cursor;
use std::io::SeekFrom;
use std::io::ErrorKind;

use std::path::Path;
use std::error;
use std::fmt;

use clap::{App, Arg};
use encoding::all::{UTF_16LE,UTF_8};
use encoding::{Encoding,EncoderTrap,DecoderTrap};
use byteorder::{LittleEndian,ReadBytesExt};


// 自定义错误类型
#[derive(Debug, Clone)]
struct WrongFileType;

impl fmt::Display for WrongFileType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "文件损坏或文件格式不对！！")
    }
}

impl error::Error for WrongFileType {
    fn description(&self) -> &str {
        "文件损坏或文件格式不对！！"
    }

    fn cause(&self) -> Option<&dyn error::Error> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}

// 词语列表元素结构
struct WordListItem {
    py_index_list: Vec<usize>,
    word: String,
    priority: usize,
}

// 文件内容结构体，方便判断文件格式
struct InputFileBuff {
    content: Vec<u8>,
    pinyin_offset: usize,
    words_offset: usize,
}

impl InputFileBuff {

    fn new(content: Vec<u8>) -> Result <InputFileBuff, Box<dyn error::Error>> {
        if content.len() < 0x2628 {
            return Err(WrongFileType.into());
        }

        if &content[0..12] != b"\x40\x15\x00\x00\x44\x43\x53\x01\x01\x00\x00\x00"{
            eprintln! ("确认你选择的是搜狗(.scel)词库?");
            return Err(WrongFileType.into());
        };

        let buff = InputFileBuff {
            content: content,
            pinyin_offset: 0x1540,
            words_offset: 0x2628,
        };
        Ok(buff)
    }
    // Rust 定义结构体时不能引用自己的成员，只能用函数的方式实现，比较蛋疼
    fn get_name(&self) -> &[u8] {
        &self.content[0x130..0x338]
    }

    fn get_type(&self) -> &[u8] {
        &self.content[0x338..0x540]
    }

    fn get_info(&self) -> &[u8] {
        &self.content[0x540..0xd40]
    }
    fn get_example(&self) -> &[u8] {
        &self.content[0xd40..0x1540]
    }

    fn get_pinyin_range(&self) -> &[u8] {
        &self.content[self.pinyin_offset..self.words_offset]
    }
    fn get_word_range(&self) -> &[u8] {
        &self.content[self.words_offset..]
    }

}

// 输入的参数选项结构
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

                if let Some(filename) = input_path.file_stem() {
                    match filename.to_str() {
                        Some(tmp_str) => format!("{}{}",tmp_str, ".txt"),
                        None=> {
                            return Err("Something wrong when due with output file!!");
                        }
                    }
                }else{
                    return Err("Something wrong when due with output file!!");
                }
            }
        };

        Ok(Config{ inputfile: input_file, outputfile: output_file})
    }
}

pub fn run(config:Config) -> Result<(), Box<dyn error::Error>> {

    // let pinyin_offset = 0x1540;
    // let words_offset = 0x2628;

    let mut infileobj = File::open(&config.inputfile)?;
    
    let mut data: Vec<u8> = Vec::new();
    infileobj.read_to_end(&mut data)?;

    let inputbuff = InputFileBuff::new(data)?;

    // 输出词库基本信息
    println!("词库名称：{}",UTF_16LE.decode(inputbuff.get_name(), DecoderTrap::Strict)?);
    println!("词库类型：{}",UTF_16LE.decode(inputbuff.get_type(), DecoderTrap::Strict)?);
    println!("词库信息：{}",UTF_16LE.decode(inputbuff.get_info(), DecoderTrap::Strict)?);
    println!("词库示例：{}",UTF_16LE.decode(inputbuff.get_example(), DecoderTrap::Strict)?);
    
    // 获取拼音表和词汇列表
    let pinyin_table = get_pinyin_table(inputbuff.get_pinyin_range())?;
    let wordlist = get_word_list(inputbuff.get_word_range())?;

    assert_eq!(String::from("zhuang"), pinyin_table[0x0191]);
    let mut outfileobj = File::create(&config.outputfile)?;

    for s in &wordlist {
        let word = &s.word;
        let pinyin_index = &s.py_index_list;
        let mut pinyin = format!("");
        
        for index in pinyin_index {
            pinyin = format!("{} {}",pinyin,pinyin_table[*index]);
        }
        let item = format!("{}\t{}\n",word,pinyin.trim()).to_string();
        outfileobj.write_all(&UTF_8.encode(&item,EncoderTrap::Strict)?)?;
    }

    Ok(())
}

fn get_pinyin_table(data: &[u8]) -> Result<Vec<String>, Box<dyn error::Error>> {

    let mut py_table: Vec<String> = Vec::new();
    if &data[0..4] != b"\x9D\x01\x00\x00" {
        return Err(WrongFileType.into());
    }

    let mut csr = Cursor::new(&data[4..]);
    loop {
        match csr.read_u16::<LittleEndian>() {
            Ok(_) => {
                let py_len = csr.read_u16::<LittleEndian>()? as usize;
                let pinyinstart = csr.position() as usize;
                let pinyin = UTF_16LE.decode(&csr.get_ref()[pinyinstart..pinyinstart+py_len], DecoderTrap::Strict)?;
                py_table.push(pinyin);
                csr.seek(SeekFrom::Current(py_len as i64))?;
            },
            Err(e) => {
                match e.kind() {
                    ErrorKind::UnexpectedEof => break,
                    _ => {
                        return Err(e.into());
                    }
                }
            }
        }
    }
    Ok(py_table)
}

fn get_word_list(data:&[u8]) -> Result<Vec<WordListItem>, Box<dyn error::Error>> {
   
    let mut word_list = Vec::new();

    let mut csr = Cursor::new(&data);
    loop {
        match csr.read_u16::<LittleEndian>() {
            Ok(mut same_num) => {
                let mut pinyin: Vec<usize> = Vec::new();
                let py_len = csr.read_u16::<LittleEndian>()? as usize;
                let current_pos = csr.position() as usize;
                while (csr.position() as usize) < current_pos +py_len {
                    let py = csr.read_u16::<LittleEndian>()? as usize;
                    pinyin.push(py);
                }

                assert_eq!(csr.position() as usize, current_pos + py_len);

                while same_num > 0 {
                    let word_len = csr.read_u16::<LittleEndian>()? as usize;
                    let wordstartpos = csr.position() as usize;
                    let word = UTF_16LE.decode(&csr.get_ref()[wordstartpos..wordstartpos+word_len], DecoderTrap::Strict)?;
                    csr.seek(SeekFrom::Current(word_len as i64))?;
                    let ext_len = csr.read_u16::<LittleEndian>()? as usize;
                    let priority = csr.read_u16::<LittleEndian>()? as usize;
                    csr.seek(SeekFrom::Current((ext_len-2) as i64))?;
                    word_list.push(WordListItem{py_index_list: pinyin.clone(), word: word, priority: priority});
                    same_num -=1;
                }
            },
            Err(e) => {
                match e.kind() {
                    ErrorKind::UnexpectedEof => break,
                    _ => {
                        return Err(e.into());
                    }
                }
            }
        }
    }
    Ok(word_list)
}


#[cfg(test)]
use std::ffi::OsString;

mod test {
    use super::*;

    #[test]
    fn testPath() {
        let path = Path::new("/home/zhenyu/touhou.scel");
        if let Some(filepath) = path.file_stem() {
            assert_eq!(filepath , &OsString::from("touhou"));
        };
    }

}