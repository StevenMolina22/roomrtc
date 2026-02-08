use crate::config::Config;
use crate::sctp_transport::data_channel::dcep::DCEPMessage;
use crate::sctp_transport::data_channel::{DataChannelError as Error, DataChannelType};
use sctp_proto::{Association, Chunks, PayloadProtocolIdentifier, StreamId};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use crate::logger::Logger;

pub struct DataChannel {
    pub(crate) stream_id: StreamId,
    pub(crate) association: Arc<Mutex<Association>>,
    logger: Arc<Logger>,
    config: Arc<Config>,
}

impl DataChannel {
    pub fn from_accepted_stream(
        stream_id: StreamId,
        association: Arc<Mutex<Association>>,
        logger: Arc<Logger>,
        config: Arc<Config>,
    ) -> Result<Self, Error> {
        let mut dc = Self {
            stream_id,
            association,
            logger,
            config,
        };

        dc.wait_for_open()?;
        dc.write_with_ppi(
            &DCEPMessage::DataChannelAck.to_bytes(),
            PayloadProtocolIdentifier::Dcep,
        )?;
        Ok(dc)
    }

    pub fn open(
        label: String,
        stream_id: StreamId,
        association: Arc<Mutex<Association>>,
        dc_type: DataChannelType,
        reliability_param: u32,
        protocol: String,
        logger: Arc<Logger>,
        config: Arc<Config>,
    ) -> Result<Self, Error> {
        let mut dc = Self {
            stream_id,
            association,
            logger,
            config,
        };
        dc.configure_stream(&label, dc_type, reliability_param, &protocol)?;

        dc.write_dcep(DCEPMessage::DataChannelOpen {
            channel_type: DataChannelType::Reliable,
            priority: 0,
            reliability_parameter: 0,
            label,
            protocol: PayloadProtocolIdentifier::Binary.to_string(),
        })?;

        dc.wait_for_ack()?;
        Ok(dc)
    }

    pub fn configure_stream(
        &mut self,
        label: &String,
        dc_type: DataChannelType,
        reliability_param: u32,
        protocol: &String,
    ) -> Result<(), Error> {
        let mut assoc = self
            .association
            .lock()
            .map_err(|e| Error::LockError(e.to_string()))?;
        let mut stream = assoc
            .stream(self.stream_id)
            .map_err(|e| Error::GetStreamError(e.to_string()))?;

        stream
            .set_default_payload_type(PayloadProtocolIdentifier::Binary)
            .map_err(|e| Error::OpenError(e.to_string()))?;
        stream
            .set_reliability_params(
                dc_type.ordered(),
                dc_type.reliability_type(),
                reliability_param,
            )
            .map_err(|e| Error::OpenError(e.to_string()))
    }

    pub fn send(&mut self, message: &[u8]) -> Result<(), Error> {
        let mut association = self
            .association
            .lock()
            .map_err(|e| Error::LockError(e.to_string()))?;
        let mut stream = association
            .stream(self.stream_id)
            .map_err(|e| Error::GetStreamError(e.to_string()))?;

        stream
            .write(message)
            .map_err(|e| Error::SendError(e.to_string()))?;
        Ok(())
    }

    pub fn recv(&mut self, buff: &mut [u8]) -> Result<usize, Error> {
        loop {
            if let Some(chunks) = self.read_chunks()? {
                return chunks
                    .read(buff)
                    .map_err(|e| Error::ReadChunksError(e.to_string()));
            }
            thread::sleep(Duration::from_millis(0));
        }
    }

    fn read_chunks(&self) -> Result<Option<Chunks>, Error> {
        let mut assoc = self
            .association
            .lock()
            .map_err(|e| Error::LockError(e.to_string()))?;
        let mut stream = assoc
            .stream(self.stream_id)
            .map_err(|e| Error::GetStreamError(e.to_string()))?;

        stream
            .read()
            .map_err(|e| Error::ReadStreamError(e.to_string()))
    }

    fn write_with_ppi(&self, bytes: &[u8], ppi: PayloadProtocolIdentifier) -> Result<(), Error> {
        let mut assoc = self
            .association
            .lock()
            .map_err(|e| Error::LockError(e.to_string()))?;
        let mut stream = assoc
            .stream(self.stream_id)
            .map_err(|e| Error::GetStreamError(e.to_string()))?;

        stream
            .write_with_ppi(bytes, ppi)
            .map_err(|e| Error::SendError(e.to_string()))?;
        Ok(())
    }

    fn write_dcep(&mut self, message: DCEPMessage) -> Result<(), Error> {
        self.write_with_ppi(&message.to_bytes(), PayloadProtocolIdentifier::Dcep)
            .map_err(|e| Error::SendError(e.to_string()))
    }

    // DCEP
    fn wait_for_ack(&self) -> Result<(), Error> {
        let clock = Instant::now();

        while !ack_timed_out(&clock, self.config.dcep.ack_wait_timeout_millis) {
            if let Some(chunks) = self.read_chunks()?
                && chunks.ppi == PayloadProtocolIdentifier::Dcep
            {
                let mut bytes = vec![0u8; 1024];
                chunks
                    .read(&mut bytes)
                    .map_err(|e| Error::ReadChunksError(e.to_string()))?;

                match DCEPMessage::from_bytes(&bytes) {
                    Some(DCEPMessage::DataChannelAck) => {
                        return Ok(())
                    },
                    _ => {}
                }
            }
        }

        Err(Error::OpenTimeout)
    }

    fn wait_for_open(&mut self) -> Result<(), Error> {
        let clock = Instant::now();

        while !open_timed_out(&clock, self.config.dcep.open_wait_timeout_millis) {
            if let Some(chunks) = self.read_chunks()?
                && chunks.ppi == PayloadProtocolIdentifier::Dcep
            {
                let mut bytes = vec![0u8; 1024];
                chunks
                    .read(&mut bytes)
                    .map_err(|e| Error::ReadChunksError(e.to_string()))?;

                match DCEPMessage::from_bytes(&bytes) {
                    Some(DCEPMessage::DataChannelOpen {
                        channel_type,
                        reliability_parameter,
                        label,
                        protocol,
                        ..
                    }) => {
                        self.configure_stream(
                            &label,
                            channel_type,
                            reliability_parameter,
                            &protocol,
                        )?;
                        return Ok(());
                    }
                    _ => {
                        continue
                    },
                }
            }
        }

        Err(Error::OpenTimeout)
    }
}

fn ack_timed_out(clock: &Instant, wait_timeout_millis: u16) -> bool {
    clock.elapsed().as_millis() > wait_timeout_millis as u128 && wait_timeout_millis > 0
}

fn open_timed_out(clock: &Instant, wait_timeout_millis: u16) -> bool {
    clock.elapsed().as_millis() > wait_timeout_millis as u128 && wait_timeout_millis > 0
}
