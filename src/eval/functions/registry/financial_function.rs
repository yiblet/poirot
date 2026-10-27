/// Cash flow, loan, and monetary formatting handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FinancialFunction {
    /// FV handler.
    Fv,
    /// PV handler.
    Pv,
    /// PMT handler.
    Pmt,
    /// NPER handler.
    Nper,
    /// IPMT handler.
    Ipmt,
    /// PPMT handler.
    Ppmt,
    /// RATE handler.
    Rate,
    /// NPV handler.
    Npv,
    /// IRR handler.
    Irr,
    /// MIRR handler.
    Mirr,
    /// DOLLAR handler.
    Dollar,
    /// DOLLARDE handler.
    DollarDe,
    /// DOLLARFR handler.
    DollarFr,
    /// FIXED handler.
    Fixed,
    /// Historical fixed-rate euro conversion.
    Euroconvert,
    /// Documented CUMIPMT calculation.
    Cumipmt,
    /// Documented CUMPRINC calculation.
    Cumprinc,
    /// Documented DB calculation.
    Db,
    /// Documented DDB calculation.
    Ddb,
    /// Documented EFFECT calculation.
    Effect,
    /// Documented FVSCHEDULE calculation.
    Fvschedule,
    /// Documented ISPMT calculation.
    Ispmt,
    /// Documented NOMINAL calculation.
    Nominal,
    /// Documented PDURATION calculation.
    Pduration,
    /// Documented RRI calculation.
    Rri,
    /// Documented SLN calculation.
    Sln,
    /// Documented SYD calculation.
    Syd,
    /// Documented VDB calculation.
    Vdb,
    /// Documented XIRR calculation.
    Xirr,
    /// Documented XNPV calculation.
    Xnpv,
    /// Documented ACCRINT security calculation.
    Accrint,
    /// Documented ACCRINTM security calculation.
    Accrintm,
    /// Documented COUPDAYBS security calculation.
    Coupdaybs,
    /// Documented COUPDAYS security calculation.
    Coupdays,
    /// Documented COUPDAYSNC security calculation.
    Coupdaysnc,
    /// Documented COUPNCD security calculation.
    Coupncd,
    /// Documented COUPNUM security calculation.
    Coupnum,
    /// Documented COUPPCD security calculation.
    Couppcd,
    /// Documented DISC security calculation.
    Disc,
    /// Documented DURATION security calculation.
    Duration,
    /// Documented INTRATE security calculation.
    Intrate,
    /// Documented MDURATION security calculation.
    Mduration,
    /// Documented PRICE security calculation.
    Price,
    /// Documented PRICEDISC security calculation.
    Pricedisc,
    /// Documented PRICEMAT security calculation.
    Pricemat,
    /// Documented RECEIVED security calculation.
    Received,
    /// Documented TBILLEQ security calculation.
    Tbilleq,
    /// Documented TBILLPRICE security calculation.
    Tbillprice,
    /// Documented TBILLYIELD security calculation.
    Tbillyield,
    /// Documented YIELD security calculation.
    Yield,
    /// Documented YIELDDISC security calculation.
    Yielddisc,
    /// Documented YIELDMAT security calculation.
    Yieldmat,
    /// Documented AMORDEGRC irregular financial calculation.
    Amordegrc,
    /// Documented AMORLINC irregular financial calculation.
    Amorlinc,
    /// Documented ODDFPRICE irregular financial calculation.
    Oddfprice,
    /// Documented ODDFYIELD irregular financial calculation.
    Oddfyield,
    /// Documented ODDLPRICE irregular financial calculation.
    Oddlprice,
    /// Documented ODDLYIELD irregular financial calculation.
    Oddlyield,
}
