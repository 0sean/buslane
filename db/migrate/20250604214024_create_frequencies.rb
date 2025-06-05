class CreateFrequencies < ActiveRecord::Migration[8.0]
  def change
    create_table :frequencies do |t|
      t.references :trip, null: false, type: :string
      t.string :start_time, null: false
      t.string :end_time, null: false
      t.integer :headway_secs, null: false
      t.integer :exact_times, default: 0

      t.timestamps
    end
  end
end
