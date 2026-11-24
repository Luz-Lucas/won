use iced_x86::{Decoder,DecoderOptions,Formatter,IntelFormatter,Instruction};
fn main(){
 let a:Vec<String>=std::env::args().collect();
 let data=std::fs::read(&a[1]).unwrap();
 let rva=u32::from_str_radix(&a[2],16).unwrap();
 let max:usize=a.get(3).map(|s|s.parse().unwrap()).unwrap_or(60);
 let pe=goblin::pe::PE::parse(&data).unwrap();
 let s=pe.sections.iter().find(|s|rva>=s.virtual_address&&rva<s.virtual_address+s.virtual_size).unwrap();
 let off=(rva-s.virtual_address+s.pointer_to_raw_data) as usize;
 let mut d=Decoder::with_ip(64,&data[off..off+0x800.min(data.len()-off)],rva as u64,DecoderOptions::NONE);
 let mut f=IntelFormatter::new(); f.options_mut().set_hex_prefix("0x"); f.options_mut().set_hex_suffix("");
 let mut i=Instruction::default(); let mut out=String::new(); let mut n=0;
 while d.can_decode()&&n<max{ d.decode_out(&mut i); out.clear(); f.format(&i,&mut out); println!("{:X}  {}",i.ip(),out); n+=1; if i.mnemonic()==iced_x86::Mnemonic::Ret && n>3 {break;} }
}
