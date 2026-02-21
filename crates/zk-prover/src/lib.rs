// crates/zk-prover/src/lib.rs

use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

/*
 * DEFINI KOLOM (Execution Trace)
 */
pub struct EviceRollupCols<T> {
    pub nonce: T,
    pub sender_balance: T,
    pub receiver_balance: T,
    pub tx_value: T,
}

/*
 * DEFINISI SIRKUIT (The AIR)
 */
pub struct EviceRollupAir;

impl<F> BaseAir<F> for EviceRollupAir {
    fn width(&self) -> usize {
        4 
    }
}

/* 
 * LOGIKA KENDALA (The Constraints)
 *
 * Di sinilah keajaiban matematika terjadi. Kita mendefinisikan aturan yang 
 * TIDAK BOLEH DILANGGAR oleh Sequencer L2.
 */
impl<AB: AirBuilder> Air<AB> for EviceRollupAir {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        
        // Ambil baris saat ini (local) dan baris berikutnya (next) dari matriks eksekusi
        let local = main.row_slice(0).unwrap();
        let next = main.row_slice(1).unwrap();

        // Jika kita berada di baris terakhir, tidak perlu membandingkan dengan baris "next"
        // builder.transition() memastikan aturan ini hanya berlaku antar-baris.
        let mut transition_builder = builder.when_transition();

        // Ambil variabel dari array berdasarkan indeks (0: nonce, 1: sender_bal, dll)
        let local_nonce = local[0].clone();
        let local_sender_bal = local[1].clone();
        let local_receiver_bal = local[2].clone();
        let local_tx_value = local[3].clone();

        let next_nonce = next[0].clone();
        let next_sender_bal = next[1].clone();
        let next_receiver_bal = next[2].clone();

        // --------------------------------------------------------------------
        // ATURAN MATEMATIKA EVICE L2 TRANSACTIONS
        // Dalam ZK AIR, kita menyatakan kebenaran dengan persamaan yang harus bernilai NOL (0).
        // Format: `A = B` ditulis sebagai `builder.assert_zero(A - B)`
        // --------------------------------------------------------------------

        // Aturan 1: Nonce pengirim harus bertambah 1 di transaksi berikutnya
        // next_nonce - (local_nonce + 1) == 0
        transition_builder.assert_eq(next_nonce, local_nonce + AB::Expr::ONE);

        // Aturan 2: Saldo pengirim harus berkurang sebesar tx_value
        // next_sender_bal - (local_sender_bal - local_tx_value) == 0
        transition_builder.assert_eq(next_sender_bal, local_sender_bal - local_tx_value.clone());

        // Aturan 3: Saldo penerima harus bertambah sebesar tx_value
        // next_receiver_bal - (local_receiver_bal + local_tx_value) == 0
        transition_builder.assert_eq(next_receiver_bal, local_receiver_bal + local_tx_value);

        // (Catatan: Pengecekan Underflow/Overflow di Plonky3 biasanya ditangani
        // dengan teknik "Range Check" menggunakan tabel Lookup, yang bisa kita
        // tambahkan nanti di tahap optimasi lanjutan).
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_air_compilation() {
        let _air = EviceRollupAir;
        assert!(true);
    }
}