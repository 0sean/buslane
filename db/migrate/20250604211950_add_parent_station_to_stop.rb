class AddParentStationToStop < ActiveRecord::Migration[8.0]
  def change
    add_reference :stops, :parent_station, null: false, foreign_key: { to_table: :stops }, type: :string
  end
end
