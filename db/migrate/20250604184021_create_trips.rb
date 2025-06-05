class CreateTrips < ActiveRecord::Migration[8.0]
  def change
    create_table :trips, id: false, force: true do |t|
      t.string :id, null: false, primary_key: true
      t.references :route, null: false
      t.references :service, null: false
      t.string :trip_headsign, default: ""
      t.integer :direction_id, null: false
      t.string :block_id, null: false
      # Shape ID
      t.boolean :wheelchair_accessible, default: false
      t.string :vehicle_journey_code, null: false

      t.timestamps
    end
  end
end
