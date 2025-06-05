class CreateStopTimes < ActiveRecord::Migration[8.0]
  def change
    create_table :stop_times do |t|
      t.references :stop, null: false, type: :string
      t.references :trip, null: false, type: :string
      t.string :arrival_time, null: false
      t.string :departure_time, null: false
      t.integer :stop_sequence
      t.string :stop_headsign
      t.integer :pickup_type, default: 0
      t.integer :drop_off_type, default: 0
      t.decimal :shape_dist_traveled
      t.integer :timepoint, default: 1

      t.timestamps
    end
  end
end
