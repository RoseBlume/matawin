use std::io::Cursor;

fn main() {
    let mut state=0xD1B5_4A32_D192_ED03u64;
    for len in 0..4096usize {
        let mut bytes=vec![0u8;len];
        for b in &mut bytes { state^=state<<13;state^=state>>7;state^=state<<17;*b=state as u8; }
        for mutation in 0..8usize {
            let mut candidate=bytes.clone();
            if !candidate.is_empty(){let i=(state as usize+mutation)%candidate.len();candidate[i]^=1u8<<(mutation&7);}
            let mut cursor=Cursor::new(candidate);
            let before=cursor.position();
            let _=mata_audio::metadata::read_all(&mut cursor);
            assert_eq!(cursor.position(),before,"metadata probe changed stream position");
        }
    }
}
