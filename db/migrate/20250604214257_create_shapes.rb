class CreateShapes < ActiveRecord::Migration[8.0]
  def change
    create_table :shapes, id: false, force: true do |t|
      t.string :id, null: false, primary_key: true
      t.string :pt_lat, null: false
      t.string :pt_lon, null: false
      t.integer :pt_sequence, null: false
      t.decimal :dist_traveled

      t.timestamps
    end
  end
end
