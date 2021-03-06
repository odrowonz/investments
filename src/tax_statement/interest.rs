use chrono::Datelike;
use static_table_derive::StaticTable;

use crate::broker_statement::BrokerStatement;
use crate::config::PortfolioConfig;
use crate::core::EmptyResult;
use crate::currency::{self, Cash, MultiCurrencyCashAccount};
use crate::currency::converter::CurrencyConverter;
use crate::formatting;
use crate::types::{Date, Decimal};

use super::statement::TaxStatement;

#[derive(StaticTable)]
struct Row {
    #[column(name="Дата")]
    date: Date,
    #[column(name="Валюта", align="center")]
    currency: String,
    #[column(name="Сумма")]
    foreign_amount: Cash,
    #[column(name="Курс руб.")]
    currency_rate: Decimal,
    #[column(name="Сумма (руб)")]
    amount: Cash,
    #[column(name="К уплате")]
    tax_to_pay: Cash,
    #[column(name="Реальный доход")]
    income: Cash,
}

pub fn process_income(
    portfolio: &PortfolioConfig, broker_statement: &BrokerStatement, year: Option<i32>,
    mut tax_statement: Option<&mut TaxStatement>, converter: &CurrencyConverter,
) -> EmptyResult {
    let mut table = Table::new();
    let country = portfolio.get_tax_country();

    let mut total_foreign_amount = MultiCurrencyCashAccount::new();
    let mut total_amount = dec!(0);
    let mut total_tax_to_pay = dec!(0);
    let mut total_income = dec!(0);

    for interest in &broker_statement.idle_cash_interest {
        if let Some(year) = year {
            if interest.date.year() != year {
                continue;
            }
        }

        let foreign_amount = interest.amount.round();
        total_foreign_amount.deposit(foreign_amount);

        let precise_currency_rate = converter.precise_currency_rate(
            interest.date, foreign_amount.currency, country.currency)?;

        let amount = currency::round(converter.convert_to(
            interest.date, interest.amount, country.currency)?);
        total_amount += amount;

        let tax_to_pay = interest.tax_to_pay(&country, converter)?;
        total_tax_to_pay += tax_to_pay;

        let income = amount - tax_to_pay;
        total_income += income;

        table.add_row(Row {
            date: interest.date,
            currency: foreign_amount.currency.to_owned(),
            foreign_amount: foreign_amount,
            currency_rate: precise_currency_rate,
            amount: Cash::new(country.currency, amount),
            tax_to_pay: Cash::new(country.currency, tax_to_pay),
            income: Cash::new(country.currency, income),
        });

        if let Some(ref mut tax_statement) = tax_statement {
            let description = format!(
                "{}: Проценты на остаток по брокерскому счету", broker_statement.broker.name);

            tax_statement.add_interest_income(
                &description, interest.date, foreign_amount.currency, precise_currency_rate,
                foreign_amount.amount, amount
            ).map_err(|e| format!(
                "Unable to add interest income from {} to the tax statement: {}",
                formatting::format_date(interest.date), e
            ))?;
        }
    }

    if !table.is_empty() {
        let mut totals = table.add_empty_row();
        totals.set_foreign_amount(total_foreign_amount);
        totals.set_amount(Cash::new(country.currency, total_amount));
        totals.set_tax_to_pay(Cash::new(country.currency, total_tax_to_pay));
        totals.set_income(Cash::new(country.currency, total_income));

        table.print(&format!(
            "Расчет дохода от процентов на остаток по брокерскому счету, полученных через {}",
            broker_statement.broker.name));
    }

    Ok(())
}