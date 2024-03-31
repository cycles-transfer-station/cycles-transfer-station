// keep 1 minute segments forever. bout 57 MiB per year

// when making this into 1-minute candlesticks, use the time_nanos as the id for an optional_start_before_id parameter since time_nanos are 1 minute between.  

use crate::*;
use cts_lib::consts::{SECONDS_IN_A_MINUTE, SECONDS_IN_A_DAY};
use std::borrow::Cow;


const MAX_CANDLES_SPONSE: usize = (MiB as usize * 1 + KiB as usize * 512) / std::mem::size_of::<Candle>(); 


#[derive(Default, CandidType, Serialize, Deserialize)]
pub struct CandleCounter {
    latest_1_minute: Candle, 
    segments_1_minute: Vec<Candle>,
    volume_cycles: Cycles,            // all-time
    volume_tokens: Tokens,            // all-time
}

impl CandleCounter {
    pub fn count_trade(&mut self, tl: &TradeLog) {
        let current_segment_start_time_nanos = segment_start_time_nanos(ViewCandlesSegmentLength::OneMinute, tl.timestamp_nanos as u64);  /*good for the next 500 years. change when nanos goes over u64::max*/
        if self.latest_1_minute.time_nanos < current_segment_start_time_nanos {
            let complete_segment: Candle = std::mem::replace(
                &mut self.latest_1_minute, 
                Candle{
                    time_nanos: current_segment_start_time_nanos,
                    volume_cycles: tl.cycles,
                    volume_tokens: tl.tokens,
                    open_rate: tl.cycles_per_token_rate,
                    high_rate: tl.cycles_per_token_rate,
                    low_rate: tl.cycles_per_token_rate,
                    close_rate: tl.cycles_per_token_rate,
                }
            ); 
            if complete_segment.time_nanos != 0 { // default latest_1_minute time_nanos is 0 so we don't use that
                self.segments_1_minute.push(complete_segment);
            }               
        } else {
            self.latest_1_minute.volume_cycles = self.latest_1_minute.volume_cycles.saturating_add(tl.cycles);
            self.latest_1_minute.volume_tokens = self.latest_1_minute.volume_tokens.saturating_add(tl.tokens);
            self.latest_1_minute.high_rate = std::cmp::max(self.latest_1_minute.high_rate, tl.cycles_per_token_rate);
            self.latest_1_minute.low_rate = std::cmp::min(self.latest_1_minute.low_rate, tl.cycles_per_token_rate);
            self.latest_1_minute.close_rate = tl.cycles_per_token_rate;
        }
        self.volume_cycles = self.volume_cycles.saturating_add(tl.cycles);
        self.volume_tokens = self.volume_tokens.saturating_add(tl.tokens);
    }
    
}


fn segment_start_time_nanos(segment_length: ViewCandlesSegmentLength, time_nanos: u64) -> u64 {
    time_nanos.saturating_sub(time_nanos % (NANOS_IN_A_SECOND as u64 * SECONDS_IN_A_MINUTE as u64 * segment_length as u64))
}



pub fn create_candles<'a>(candle_counter: &'a CandleCounter, q: ViewCandlesQuest) -> Cow<'a, [Candle]> {
        
    let mut s = &candle_counter.segments_1_minute[..];
    
    let mut candles: VecDeque<Candle> = VecDeque::new();  
    
    fn candles_push_front_calibrate_segment_start_time(candles: &mut VecDeque<Candle>, c: &Candle, segment_length: ViewCandlesSegmentLength) {
        candles.push_front(Candle{
            time_nanos: segment_start_time_nanos(segment_length, c.time_nanos),
            ..c.clone()    
        });
    }
    
    if let Some(start_before_time_nanos) = q.opt_start_before_time_nanos {
        let start_before_segment_start_time_nanos = segment_start_time_nanos(q.segment_length, start_before_time_nanos); 
        s = &s[..s.binary_search_by_key(&start_before_segment_start_time_nanos, |c| { c.time_nanos }).unwrap_or_else(|e| e)];
        
        if candle_counter.latest_1_minute.time_nanos < start_before_segment_start_time_nanos {
            candles_push_front_calibrate_segment_start_time(&mut candles, &candle_counter.latest_1_minute, q.segment_length);
        }
    } else {
        candles_push_front_calibrate_segment_start_time(&mut candles, &candle_counter.latest_1_minute, q.segment_length);
    }
    
    if s.len() == 0 {
        return Cow::Owned(candles.into_iter().collect::<Vec<Candle>>());
    }
        
    if q.segment_length == ViewCandlesSegmentLength::OneMinute {
        if candles.len() == 0 {
        	return Cow::Borrowed(s.rchunks(MAX_CANDLES_SPONSE).next().unwrap());
    	}
    }
    
    let mut iter = s.iter().rev();
    
    if candles.len() == 0 {
        candles_push_front_calibrate_segment_start_time(&mut candles, iter.next().unwrap(), q.segment_length);
    }
    
    for c in iter {
        let latest_candle: &mut Candle = candles.front_mut().unwrap();
        let c_segment_start_time_nanos = segment_start_time_nanos(q.segment_length, c.time_nanos);
        if c_segment_start_time_nanos < latest_candle.time_nanos {
            candles.push_front(Candle{
                time_nanos: c_segment_start_time_nanos,
                ..c.clone()
            });
        } else {
            latest_candle.volume_cycles = latest_candle.volume_cycles.saturating_add(c.volume_cycles);
            latest_candle.volume_tokens = latest_candle.volume_tokens.saturating_add(c.volume_tokens);
            latest_candle.open_rate = c.open_rate; // since we are moving backwards
            latest_candle.high_rate = std::cmp::max(latest_candle.high_rate, c.high_rate);
            latest_candle.low_rate = std::cmp::min(latest_candle.low_rate, c.low_rate);
        }
        
        if candles.len() >= MAX_CANDLES_SPONSE {
            break;
        }
    }
    
    Cow::Owned(candles.into_iter().collect::<Vec<Candle>>())
}





#[derive(CandidType, Deserialize)]
pub struct ViewVolumeStatsSponse {
    volume_cycles: Volume,
    volume_tokens: Volume,
}
#[derive(CandidType, Deserialize)]
pub struct Volume{
    volume_24_hour: u128,
    volume_7_day: u128,
    volume_30_day: u128,
    volume_sum: u128,
}



pub fn create_view_volume_stats(candle_counter: &CandleCounter) -> ViewVolumeStatsSponse {
    
    let h = |timeframe_length_nanos: u128| {
        let timeframe_start_nanos = time_nanos_u64().saturating_sub(timeframe_length_nanos as u64);
    
        let start_count: (Cycles, Tokens) = if candle_counter.latest_1_minute.time_nanos >= timeframe_start_nanos {
            (candle_counter.latest_1_minute.volume_cycles, candle_counter.latest_1_minute.volume_tokens)
        } else {
            (0, 0)
        };
        
        candle_counter.segments_1_minute[
            candle_counter.segments_1_minute.binary_search_by_key(&timeframe_start_nanos, |c| c.time_nanos).unwrap_or_else(|e| e)            
            ..
        ]
        .iter()
        .fold(start_count, |(count_cycles, count_tokens), c| {
            (count_cycles.saturating_add(c.volume_cycles), count_tokens.saturating_add(c.volume_tokens))            
        })
    };
    
    let (vc_24_hour, vt_24_hour) = h(NANOS_IN_A_SECOND * SECONDS_IN_A_DAY * 1); 
    let (vc_7_day,   vt_7_day)   = h(NANOS_IN_A_SECOND * SECONDS_IN_A_DAY * 7);
    let (vc_30_day,  vt_30_day)  = h(NANOS_IN_A_SECOND * SECONDS_IN_A_DAY * 30); 
    
    ViewVolumeStatsSponse {
        volume_cycles: Volume{
            volume_24_hour: vc_24_hour,
            volume_7_day: vc_7_day,
            volume_30_day: vc_30_day,
            volume_sum: candle_counter.volume_cycles,
        },
        volume_tokens: Volume{
            volume_24_hour: vt_24_hour,
            volume_7_day: vt_7_day,
            volume_30_day: vt_30_day,
            volume_sum: candle_counter.volume_tokens,
        },
    }    
}













