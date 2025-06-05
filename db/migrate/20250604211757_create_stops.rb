class CreateStops < ActiveRecord::Migration[8.0]
  def change
    create_table :stops, id: false, force: true do |t|
      t.string :id, null: false, primary_key: true
      t.string :code, null: false
      t.string :name, null: false
      t.decimal :lat, null: false
      t.decimal :lon, null: false
      t.boolean :wheelchair_boarding, default: false
      t.integer :location_type, null: false, default: 0
      t.string :platform_code

      t.timestamps
    end
  end
end
