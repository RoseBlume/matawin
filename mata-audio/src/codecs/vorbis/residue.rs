use crate::{AudioError, codecs::support::StageTimer};
use super::{bit_reader::BitReader, codebook::Codebook, header::Residue};

#[derive(Debug, Default, Clone)]
pub(super) struct ResidueScratch {
    classifications: Vec<usize>,
    combined: Vec<f32>,
}
impl ResidueScratch {
    pub(super) fn retained_bytes(&self)->usize{self.classifications.capacity()*core::mem::size_of::<usize>()+self.combined.capacity()*core::mem::size_of::<f32>()}
    pub(super) fn trim_memory(&mut self){self.classifications.shrink_to_fit();self.combined.shrink_to_fit();}
}

/// Decode residue directly into a flat channel-major arena.
pub(super) fn decode_residue_into(
    bits:&mut BitReader<'_>,block_size:usize,do_not_decode:&[bool],residue:&Residue,codebooks:&[Codebook],
    output:&mut [f32],scratch:&mut ResidueScratch,
)->Result<(),AudioError>{
    let _stage_timer = StageTimer::start(match residue.residue_type { 0 => "vorbis.residue.type0", 1 => "vorbis.residue.type1", 2 => "vorbis.residue.type2", _ => "vorbis.residue.invalid" });
    let channels=do_not_decode.len();let vector_size=block_size/2;
    if output.len()!=channels.saturating_mul(vector_size){return Err(AudioError::InvalidFormat("Vorbis residue arena has wrong size".into()));}
    output.fill(0.0);
    if residue.residue_type==2{
        if do_not_decode.iter().all(|&skip|skip){return Ok(());}
        let combined_size=vector_size.checked_mul(channels).ok_or_else(||AudioError::InvalidFormat("Vorbis residue-2 vector size overflow".into()))?;
        scratch.combined.resize(combined_size,0.0);scratch.combined.fill(0.0);
        decode_inner_into(bits,combined_size,&[false],residue,codebooks,&mut scratch.combined,&mut scratch.classifications)?;
        // Residue type 2's scalar vector is interleaved by channel. Convert it
        // directly into the channel-major arena in one bounded traversal.
        for sample in 0..vector_size{for channel in 0..channels{output[channel*vector_size+sample]=scratch.combined[sample*channels+channel];}}
        Ok(())
    }else{
        decode_inner_into(bits,vector_size,do_not_decode,residue,codebooks,output,&mut scratch.classifications)
    }
}

fn decode_inner_into(
    bits:&mut BitReader<'_>,actual_size:usize,do_not_decode:&[bool],residue:&Residue,codebooks:&[Codebook],vectors:&mut[f32],classifications:&mut Vec<usize>,
)->Result<(),AudioError>{
    let channels=do_not_decode.len();let begin=residue.begin.min(actual_size);let end=residue.end.min(actual_size);if end<=begin{return Ok(());}
    let classbook=codebooks.get(residue.classbook).ok_or_else(||AudioError::InvalidFormat("Vorbis residue classbook is out of range".into()))?;
    let classwords=classbook.dimensions as usize;if classwords==0||residue.partition_size==0||residue.classifications==0{return Err(AudioError::InvalidFormat("Invalid Vorbis residue setup".into()));}
    let partitions=(end-begin)/residue.partition_size;if partitions==0{return Ok(());}let stride=partitions.checked_add(classwords).ok_or_else(||AudioError::InvalidFormat("Vorbis residue classification size overflow".into()))?;
    let class_len=channels.checked_mul(stride).ok_or_else(||AudioError::InvalidFormat("Vorbis residue classification buffer overflow".into()))?;classifications.resize(class_len,0);
    'decode:for pass in 0..8usize{let mut partition=0usize;while partition<partitions{
        if pass==0{for channel in 0..channels{if do_not_decode[channel]{continue;}let mut value=match classbook.decode_scalar(bits){Ok(v)=>v as usize,Err(AudioError::TruncatedData)=>break 'decode,Err(e)=>return Err(e)};for word in(0..classwords).rev(){let at=partition+word;if at<stride{classifications[channel*stride+at]=value%residue.classifications;}value/=residue.classifications;}}}
        for _ in 0..classwords{if partition>=partitions{break;}for channel in 0..channels{if do_not_decode[channel]{continue;}let class=classifications[channel*stride+partition];let Some(book_index)=residue.books[class][pass]else{continue;};let book=&codebooks[book_index];let offset=begin+partition*residue.partition_size;let end_offset=(offset+residue.partition_size).min(actual_size);if offset>=end_offset{continue;}let base=channel*actual_size;let result=read_partition(bits,book,residue.residue_type,&mut vectors[base+offset..base+end_offset]);match result{Ok(())=>{},Err(AudioError::TruncatedData)=>break 'decode,Err(e)=>return Err(e)}}partition+=1;}
    }}Ok(())
}

fn read_partition(bits:&mut BitReader<'_>,codebook:&Codebook,residue_type:u8,output:&mut[f32])->Result<(),AudioError>{
    if residue_type==0{let dimensions=codebook.dimensions as usize;if dimensions==0{return Err(AudioError::InvalidFormat("Vorbis residue codebook has zero dimensions".into()));}let step=output.len()/dimensions;for i in 0..step{let vector=codebook.decode_vector(bits)?;for(j,&value)in vector.iter().enumerate(){let at=i+j*step;if at<output.len(){output[at]+=value;}}}}
    else{let mut i=0usize;while i<output.len(){let vector=codebook.decode_vector(bits)?;if vector.is_empty(){return Err(AudioError::InvalidFormat("Vorbis residue VQ codebook has an empty vector".into()));}let take=(output.len()-i).min(vector.len());for(dst,&value)in output[i..i+take].iter_mut().zip(&vector[..take]){*dst+=value;}i+=take;}}
    Ok(())
}
