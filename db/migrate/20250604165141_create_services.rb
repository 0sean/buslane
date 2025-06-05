class CreateServices < ActiveRecord::Migration[8.0]
  def change
    create_table :services, id: false, force: true do |t|
      t.integer :id, null: false, primary_key: true
      t.boolean :monday, default: false
      t.boolean :tuesday, default: false
      t.boolean :wednesday, default: false
      t.boolean :thursday, default: false
      t.boolean :friday, default: false
      t.boolean :saturday, default: false
      t.boolean :sunday, default: false
      t.date :start_date
      t.date :end_date

      t.timestamps
    end
  end
end
